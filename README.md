# Slow SchemaRead compilation for nested generic types

We encountered a substantial increase in compilation time while migrating a
codebase from bincode to wincode. The bincode implementation did not show
comparable compile-time issues with a similar hierarchy of nested generic types.
Replacing some derived `SchemaRead` implementations with handwritten readers
using simpler trait bounds substantially reduced the compilation time.

This standalone crate reproduces the behavior with synthetic types and no
application dependencies or data files. It compares a derived reader with two
handwritten readers whose bodies and metadata are identical, but whose trait
bounds differ. The measurements below are for this wincode-only reproduction.

The crate pins **wincode 0.6.3** and includes a lockfile resolving
**wincode-derive 0.5.1**. It is a standalone Cargo workspace and requires no
nightly features.

## Reproduce

From this directory, run:

```sh
# Derived outer reader.
CARGO_INCREMENTAL=0 cargo check --locked --timings

# Handwritten outer reader with a bound on its nested field type.
CARGO_INCREMENTAL=0 cargo check --locked --timings --features field-bounds

# Same handwritten reader, with a bound on the scalar type instead.
CARGO_INCREMENTAL=0 cargo check --locked --timings --features manual
```

These commands use nesting depth 12. For depth 14, use `--features deep`,
`--features deep,field-bounds`, and `--features deep,manual`, respectively.
The slower depth-14 cases take about 33 seconds on the machine described below.
The `manual` and `field-bounds` features are mutually exclusive, so
`--all-features` is not supported.

Cargo skips unchanged builds. Touch `src/lib.rs` before repeating a command,
or use the included script to collect repeated measurements:

```sh
python3 measure.py --repeat 2
python3 measure.py --deep --repeat 2
```

The script warms dependencies, disables incremental compilation, touches the
source before each sample, and alternates mode order between repetitions. It
records wall time and the repro crate's own Cargo timing separately. Results,
logs, and HTML timing reports are written to `results/depth-12` and
`results/depth-14`. Add `--offline` if dependencies are already cached, and run
without other concurrent builds.

## Results

Measured with stable rustc 1.99.1 (`58cf8f9c3`) on Linux x86_64, using an AMD
Ryzen 7 7840U. Values are means of two runs, with incremental compilation off,
and exclude dependency compilation and Cargo overhead.

| Nesting depth | Derived reader | Handwritten field bound | Handwritten scalar bound |
| --- | ---: | ---: | ---: |
| 12 | 2.75 s | 2.72 s | 1.27 s |
| 14 | 32.73 s | 32.56 s | 9.81 s |

Adding two nesting levels increases the derived reader's checking time from
2.75 s to 32.73 s. At depth 14, changing only the handwritten reader's bound
reduces checking time by about 70% (3.3× faster).

The recorded samples and compiler details are in
[measurements-depth-12.json](measurements-depth-12.json) and
[measurements-depth-14.json](measurements-depth-14.json). These are `cargo check`
measurements: no application code runs, and there is no LLVM code generation.
Cargo attributes almost all the measured time to the compiler frontend.

## What the example isolates

In [src/lib.rs](src/lib.rs), `Leaf<'a, T>` contains a `T` and
`PhantomData<&'a ()>`. Each derived `LevelN` contains two fields of type
`LevelN-1`. `Root` contains one field of the selected nesting level. The
concrete `decode` function requests `Root<'_, f32>`; removing that function
makes the generic declarations compile quickly.

All nested readers remain derived. Only the outer `Root` reader changes:

| Feature | Outer implementation |
| --- | --- |
| None | `#[derive(SchemaRead)]` |
| `field-bounds` | Handwritten, with `Inner<'de, T>: SchemaRead<'de, C, Dst = Inner<'de, T>>` |
| `manual` | Handwritten, with `T: SchemaRead<'de, C, Dst = T>` |

The two handwritten implementations expand from the same macro and differ
only in their where-clause. Both retain generic configuration and scalar types,
identical static-size metadata with zero-copy disabled, and in-place decoding.
There are no serialization derives in the example.

Increasing depth also increases the nested type's size. Comparisons within a
depth use exactly the same type, so the field-bound versus scalar-bound
comparison isolates the effect of the outer reader's bounds.

## Possible investigation direction

The handwritten field-bound implementation takes approximately as long to
check as the derive. Giving the same implementation a scalar bound makes it
substantially faster. This suggests investigating the generated nested
`SchemaRead<..., Dst = FieldType>` predicates and their interaction with
rustc's trait solver and associated-type normalization.

In wincode-derive 0.5.1, `schema_read::append_where_clause` adds constraints for
generic fields and fields containing lifetimes, and `Field::as_constraint`
includes the `Dst` equality. These may be useful starting points for exploring
whether simpler generated bounds could improve compilation time.

The measurements do not identify a particular compiler query as the cause.
Some field bounds are necessary for reference and adapter types, so this
example does not establish that they can be removed universally. The best fix
may involve the derive, rustc, or both.

## Correctness checks

```sh
cargo test --locked
cargo test --locked --features field-bounds
cargo test --locked --features manual
```

All three modes pass the included test, which checks serialized-size metadata,
successful decoding of a known-length input, and rejection of truncated input.
The measurement script uses `cargo check --lib`, excluding test compilation
and execution.
