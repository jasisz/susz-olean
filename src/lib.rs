//! `.olean` files straight to Susz: olean-export's records handed to
//! susz's packer one by one, with no ndjson in between.
//!
//! The file is byte for byte what `olean-export -f ndjson | susz pack`
//! writes, but for one thing: `mdata` records, which olean-export writes
//! without the `data` key that `susz pack` requires (their data is not kept
//! either way).

use olean_export::{
    record::{self as rec, Decl as ODecl, Head as OHead, Safety as OSafety},
    Binder as OBinder, Env, Exporter, Hints as OHints, QuotKind as OQuot, Record, Sink,
};
use std::{cell::RefCell, rc::Rc};
use susz::pack::{
    Binder, Decl, Head, Hints, IndCtor, IndRec, IndRule, IndType, Options, PackError, Packer,
    QuotKind, Safety, Stats,
};

/// A [`Sink`] packing every record it takes into a Susz file.
pub struct SuszSink {
    p: Packer,
    // the packer's own error, kept with its fault (olean-export's error type
    // carries only a message), shared with whoever drives the exporter
    err: Rc<RefCell<Option<PackError>>>,
}

/// What can go wrong between olean files and a Susz file.
#[derive(Debug)]
pub enum Failure {
    /// Reading the olean files (olean-export).
    Olean(olean_export::Error),
    /// Packing (susz): an invalid export or one this packer cannot pack.
    Pack(PackError),
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Failure::Olean(e) => write!(f, "olean-export: {e}"),
            Failure::Pack(e) => write!(f, "susz: {e}"),
        }
    }
}

impl std::error::Error for Failure {}

impl SuszSink {
    /// Room for about `exprs` expressions up front.
    pub fn with_capacity(exprs: usize) -> SuszSink {
        SuszSink {
            p: Packer::with_capacity(exprs),
            err: Rc::default(),
        }
    }

    /// Where the packer's error lands when [`Sink::record`] fails on one.
    pub fn error_slot(&self) -> Rc<RefCell<Option<PackError>>> {
        Rc::clone(&self.err)
    }

    fn take(&mut self, id: Option<u32>, r: &Record<'_>) -> Result<(), PackError> {
        let p = &mut self.p;
        let id = id.map(u64::from);
        let need = || PackError::cannot("internal: a record without its id");
        let x = u64::from;
        match r {
            Record::Meta(json) => p.meta(json),
            Record::NameStr { pre, str } => p.name_str(id.ok_or_else(need)?, x(*pre), str),
            Record::NameNum { pre, i } => p.name_num(id.ok_or_else(need)?, x(*pre), *i),
            Record::Succ(a) => p.succ(id.ok_or_else(need)?, x(*a)),
            Record::Max(a, b) => p.max(id.ok_or_else(need)?, x(*a), x(*b)),
            Record::IMax(a, b) => p.imax(id.ok_or_else(need)?, x(*a), x(*b)),
            Record::Param(n) => p.param(id.ok_or_else(need)?, x(*n)),
            Record::BVar(i) => p.bvar(id.ok_or_else(need)?, *i),
            Record::Sort(l) => p.sort(id.ok_or_else(need)?, x(*l)),
            Record::Const { name, us } => {
                let us: Vec<u64> = us.iter().map(|&u| x(u)).collect();
                p.constant(id.ok_or_else(need)?, x(*name), &us)
            }
            Record::App { fun, arg } => p.app(id.ok_or_else(need)?, x(*fun), x(*arg)),
            Record::Lam(b) => p.lam(
                id.ok_or_else(need)?,
                binder(b.info),
                x(b.name),
                x(b.ty),
                x(b.body),
            ),
            Record::Pi(b) => p.pi(
                id.ok_or_else(need)?,
                binder(b.info),
                x(b.name),
                x(b.ty),
                x(b.body),
            ),
            Record::Let {
                name,
                ty,
                value,
                body,
                ..
            } => p.let_e(id.ok_or_else(need)?, x(*name), x(*ty), x(*value), x(*body)),
            // olean-export holds literals in decimal (it converts the olean's
            // limbs when it reads them): one conversion back here
            Record::Nat(d) => p.nat_decimal(id.ok_or_else(need)?, d),
            Record::Str(s) => p.str_lit(id.ok_or_else(need)?, s),
            Record::Proj {
                type_name,
                idx,
                struct_,
            } => p.proj(id.ok_or_else(need)?, x(*type_name), *idx, x(*struct_)),
            Record::MData(e) => p.mdata(id.ok_or_else(need)?, x(*e)),
            Record::Decl(d) => p.decl(&decl(d)?),
            Record::End(_) => Ok(()),
        }
    }
}

impl Sink for SuszSink {
    type Output = Packer;

    fn record(&mut self, id: Option<u32>, r: &Record<'_>) -> olean_export::Result<()> {
        if self.err.borrow().is_some() {
            return Err(olean_export::Error::Misuse(
                "a record after a packing error",
            ));
        }
        self.take(id, r).map_err(|e| {
            let msg = e.msg.clone();
            *self.err.borrow_mut() = Some(e);
            olean_export::Error::Corrupt(format!("susz: {msg}"))
        })
    }

    fn finish(self) -> olean_export::Result<Packer> {
        Ok(self.p)
    }
}

fn binder(b: OBinder) -> Binder {
    match b {
        OBinder::Default => Binder::Default,
        OBinder::Implicit => Binder::Implicit,
        OBinder::StrictImplicit => Binder::StrictImplicit,
        OBinder::InstImplicit => Binder::InstImplicit,
    }
}

fn ids(v: &[u32]) -> Vec<u64> {
    v.iter().map(|&x| u64::from(x)).collect()
}

fn head(h: &OHead) -> Head {
    Head {
        name: h.name.into(),
        level_params: ids(&h.level_params),
        ty: h.ty.into(),
    }
}

// What `susz pack` would make of the declaration's ndjson line: axioms
// written with `all`, or opaques without, are lines it does not take.
fn decl(d: &ODecl) -> Result<Decl, PackError> {
    Ok(match d {
        ODecl::Axiom {
            head: h,
            is_unsafe,
            all,
        } => {
            if all.is_some() {
                return Err(PackError::cannot("unknown key 'all'"));
            }
            Decl::Axiom {
                head: head(h),
                is_unsafe: *is_unsafe,
            }
        }
        ODecl::Def {
            head: h,
            value,
            hints,
            safety,
            all,
        } => Decl::Def {
            head: head(h),
            value: (*value).into(),
            hints: match hints {
                OHints::Opaque => Hints::Opaque,
                OHints::Abbrev => Hints::Abbrev,
                OHints::Regular(n) => Hints::Regular(u64::from(*n)),
            },
            safety: match safety {
                OSafety::Safe => Safety::Safe,
                OSafety::Unsafe => Safety::Unsafe,
                OSafety::Partial => Safety::Partial,
            },
            all: ids(all),
        },
        ODecl::Thm {
            head: h,
            value,
            all,
        } => Decl::Thm {
            head: head(h),
            value: (*value).into(),
            all: ids(all),
        },
        ODecl::Opaque {
            head: h,
            value,
            is_unsafe,
            all,
        } => Decl::Opaque {
            head: head(h),
            value: (*value).into(),
            is_unsafe: *is_unsafe,
            all: ids(all
                .as_ref()
                .ok_or_else(|| PackError::invalid("missing key 'all'"))?),
        },
        ODecl::Quot { head: h, kind } => Decl::Quot {
            head: head(h),
            kind: match kind {
                OQuot::Type => QuotKind::Type,
                OQuot::Ctor => QuotKind::Ctor,
                OQuot::Lift => QuotKind::Lift,
                OQuot::Ind => QuotKind::Ind,
            },
        },
        ODecl::Inductive { types, ctors, recs } => Decl::Inductive {
            types: types.iter().map(ind_type).collect(),
            ctors: ctors.iter().map(ind_ctor).collect(),
            recs: recs.iter().map(ind_rec).collect(),
        },
    })
}

fn ind_type(t: &rec::IndType) -> IndType {
    IndType {
        head: head(&t.head),
        is_unsafe: t.is_unsafe,
        num_params: t.num_params,
        num_indices: t.num_indices,
        all: ids(&t.all),
        ctors: ids(&t.ctors),
        num_nested: t.num_nested,
        is_rec: t.is_rec,
        is_reflexive: t.is_reflexive,
    }
}

fn ind_ctor(c: &rec::IndCtor) -> IndCtor {
    IndCtor {
        head: head(&c.head),
        is_unsafe: c.is_unsafe,
        induct: c.induct.into(),
        cidx: c.cidx,
        num_params: c.num_params,
        num_fields: c.num_fields,
    }
}

fn ind_rec(r: &rec::IndRec) -> IndRec {
    IndRec {
        head: head(&r.head),
        is_unsafe: r.is_unsafe,
        all: ids(&r.all),
        num_params: r.num_params,
        num_indices: r.num_indices,
        num_motives: r.num_motives,
        num_minors: r.num_minors,
        rules: r
            .rules
            .iter()
            .map(|x| IndRule {
                ctor: x.ctor.into(),
                nfields: x.nfields,
                rhs: x.rhs.into(),
            })
            .collect(),
        k: r.k,
    }
}

/// Every non-internal constant of `env`, in load order (as `olean-export`
/// with no constant named), packed.
pub fn pack_env(env: &Env) -> Result<Packer, Failure> {
    let sink = SuszSink::with_capacity(env.exprs.len());
    let slot = sink.error_slot();
    let mut ex = Exporter::with_sink(env, sink);
    let run = ex.meta().and_then(|()| ex.all());
    let fail = |e| match slot.borrow_mut().take() {
        Some(p) => Failure::Pack(p),
        None => Failure::Olean(e),
    };
    run.map_err(fail)?;
    ex.finish().map_err(fail)
}

/// [`pack_env`] written to `output` (under a temporary name, renamed when
/// complete).
pub fn write_env(env: &Env, output: &str, opts: &Options) -> Result<Stats, Failure> {
    pack_env(env)?
        .write_file(output, opts)
        .map_err(Failure::Pack)
}
