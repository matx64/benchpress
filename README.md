# 🏋️ BenchPress

A lightweight HTTP benchmarking tool.

## Usage

```
Usage: benchpress [OPTIONS] --url <URL>

Options:
  -u, --url <URL>                  Target URL to benchmark
  -r, --requests <REQUESTS>        Total number of requests [default: 1]
  -c, --concurrency <CONCURRENCY>  Number of concurrent requests [default: 50]
  -t, --timeout <TIMEOUT>          Request timeout in seconds [default: 30]
  -m, --method <METHOD>            HTTP method [default: GET] [possible values: get, post, put, delete, head, options, patch]
  -h, --help                       Print help
  -V, --version                    Print version
```

## Example

```sh
  cargo run --release -- -u http://localhost:3000 -r 100000 -c 1000 -t 10 -m get
```
