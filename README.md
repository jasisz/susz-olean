# susz-olean

Lean 4 `.olean` files straight to a [Susz](https://github.com/jasisz/susz)
file, with no ndjson in between.

```sh
susz-olean [-L DIR]... MODULE... -o OUT.susz
```

It loads `MODULE` and its imports from the `.olean` files found on `-L DIR`,
`LEAN_PATH` and `LEAN_SYSROOT`, and packs every non-internal constant into
`OUT.susz`. The file is the one

```sh
olean-export MODULE... -f ndjson | susz pack
```

writes, byte for byte: [olean-export](https://github.com/sdiehl/olean-export)
reads the `.olean` files and hands each record to susz's
[`Packer`](https://docs.rs/susz/latest/susz/pack/struct.Packer.html), which
packs records one at a time instead of parsing them back out of JSON.

Under `lake env` the search path is already set:

```sh
lake env susz-olean Mathlib -o mathlib.susz
```

Options: `--jobs N` threads decoding the `.olean` files and `--threads N`
threads writing the Susz file (both all cores by default), `--no-index`
leaves out the INDEX section, `--quiet` drops the summary line. Exit codes
are susz's: 0 written, 1 the export is not valid, 2 cannot (the `.olean`
files, I/O, a limit).

## Install

```sh
cargo install susz-olean
```

It needs Rust 1.100 or later (olean-export's minimum).

## Lean versions

The ones olean-export 0.8 reads: Lean 4.26 to 4.36. The `.olean` files must
come from one of those, all from the same one.

## How it was checked

- On all of Mathlib, the file is byte-identical to `olean-export -f ndjson`
  followed by `susz pack`.
- On Init, `susz digest --kernel --each` gives the same digest for every
  declaration as the file packed from lean4export's export of the same
  modules: the kernel sees the same declarations either way.
- Gruszka, an independent Lean kernel reading Susz, accepts the files.

What it costs on Mathlib (Lean 4.33.1), against the ndjson route, with
`--jobs 4 --threads 4` on both:

| | instructions | wall | peak RSS |
| --- | --- | --- | --- |
| `susz-olean` | 370 G | 45.5 s | 4.78 GB |
| `olean-export -f ndjson` | 340 G | 40.7 s | 3.27 GB |
| then `susz pack` | 38.6 G | 4.8 s | 2.22 GB |

The point is not speed but one step instead of two, and no 6 GB ndjson file
on the disk in between.

## Library

`susz_olean::pack_env` packs an `olean_export::Env` into a
`susz::pack::Packer`, and `susz_olean::write_env` writes it to a file.
`susz_olean::SuszSink` is the olean-export `Sink` underneath, for driving
the exporter yourself (some constants only, say).

## License

MIT or Apache 2.0, at your option. See [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE).

olean-export currently depends on malachite (LGPL-3.0); [sdiehl/olean-export#12](https://github.com/sdiehl/olean-export/pull/12) replaces it. Until it lands, binaries built from this crate link LGPL code.
