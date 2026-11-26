# Methodology

This page describes what a Benchpress run measures and how the site turns runs into the numbers it shows.

## What runs

A suite pins one Rust project at an exact commit and one exact toolchain version. The CLI downloads the project's source archive and checks its SHA-256 against the manifest. It installs the toolchain into its own rustup home under the Benchpress cache, so the run never uses or changes your toolchains. Build directories live outside your home directory and are removed after every run, including after Ctrl-C. [Local data](../README.md#local-data) lists where everything goes and how to remove it. `cargo fetch` downloads dependencies before any timing starts, and every timed build passes `--locked --offline`.

Each suite has five scenarios:

| Scenario | Command | Starting state |
| --- | --- | --- |
| Clean debug | `cargo build` | empty target directory |
| Incremental leaf | `cargo build` | after a clean debug build, with a comment appended to a file in the binary crate |
| Incremental core | `cargo build` | same, with the edit in a library most of the workspace depends on |
| Clean release | `cargo build --release` | empty target directory |
| Check | `cargo check` | empty target directory |

Incremental scenarios append a line such as `// benchpress 2` to the touched file, so both cargo and rustc's incremental cache see a real change. The CLI restores the original file after every run, including after Ctrl-C.

Each scenario runs three times (the `[runs]` table in the manifest). The quick suite also runs one unmeasured warmup build first. The CLI waits 10 seconds between runs by default (`--cooldown`).

## Isolation

Cargo and rustc read settings from the environment and from config files, and any of them can change build time. The CLI starts cargo with an empty environment and adds back only `PATH` (the pinned toolchain first), `HOME`, `TMPDIR`, `LANG=C` and `TERM`. `CARGO_HOME` points at a cargo home that Benchpress owns, and it deletes any `config.toml` it finds there. Variables such as `RUSTFLAGS`, `RUSTC_WRAPPER` and `CARGO_TARGET_DIR` never reach the build. The result lists the names of the variables it cleared, never their values. The work directory lives outside your home directory, so cargo config files in parent directories do not apply.

## Measurement

- Wall time is the elapsed time of the cargo process, measured with a monotonic clock.
- CPU time is user plus system time of cargo and every process it waited for, from `getrusage(RUSAGE_CHILDREN)` before and after the run.
- CPU used is CPU time divided by wall time and the number of logical cores. 100% means every core was busy for the whole build.
- A sampler thread reads system memory, swap and, on Linux, memory pressure (PSI) every 250 ms. Peak memory is the highest system memory use during the run minus the level just before it started, so it includes everything the build spawned.
- The run records power source, low power mode, thermal state on macOS and the CPU governor on Linux.

The summary for a scenario is the median wall time with the minimum, maximum and coefficient of variation (sample standard deviation divided by the mean).

## Flags

The server accepts results taken in poor conditions but marks them. Charts and rankings leave flagged results out unless you tick "Include flagged".

| Flag | When |
| --- | --- |
| Battery | the machine ran on battery power |
| Low power | low power mode or a power-saving profile was active |
| Virtualized | the CLI detected a virtual machine or container |
| High variance | a scenario's coefficient of variation is above 10% |
| Outlier | a scenario's median is more than 1.5 times faster or slower than the median of at least four other unflagged results with the same suite version, CPU family, RAM size and OS, and at least two thirds of those results lie within 1.5 times of the peers' median. The server recomputes this flag whenever results change and never stores it, so the result rejoins comparisons once enough results agree with it |

## Validation

The server rejects a result when its suite hash does not match the suite it names, when a scenario is missing or has too few runs, when a run failed, or when numbers are implausible: wall time under the scenario's floor, CPU use above 110%, memory use above RAM plus swap, or a summary that does not match its runs. The server recomputes every summary from the raw runs and rejects the result on any difference. It also rejects a result whose client build target does not match the OS and CPU architecture it reports.

## Integrity

Results come from machines the server does not control, so nothing can prove that a submitted number came from a real run. Anyone can modify the open-source client or post JSON by hand. Benchpress makes fake results costly to submit and limits what they can change.

- Validation rejects documents that no real run could produce, as described above.
- Outlier detection holds back a result that disagrees with its configuration's peers when at least two thirds of those peers agree with each other, so one fake result cannot move a configuration's numbers once it has a few honest results. Many coordinated fakes still can. Medians, not means, drive every chart and ranking.
- Each network (an IPv4 address or an IPv6 /64) can submit 10 results in a burst, then one every six minutes, and an identical document is accepted only once.
- The operator can unpublish any result with the admin token.

A configuration with fewer than five unflagged results has no peers to compare against, so treat its numbers as provisional.

## Comparisons

The site groups results into configurations: the same CPU model, memory size (rounded to a standard size) and operating system. A configuration's time is the median of its results' medians. Spreads on the machines page show the 25th to 75th percentile.

## What a result contains

CPU model and core counts, total memory and swap, storage type and filesystem, machine vendor, model and form factor, OS name and version, kernel version, power and thermal state, the toolchain and suite version, and the timings above. It contains no username, hostname, serial number, IP address or file path. The server does not log IP addresses. Rate limiting keeps counts per IP in memory only.
