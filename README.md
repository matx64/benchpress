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
  -H, --header <KEY=VALUE>         Custom HTTP header [allows multiple]
  -h, --help                       Print help
  -V, --version                    Print version
```

## Examples

```sh
  cargo run --release -- -u http://localhost:3000 -r 1000
  cargo run --release -- -u http://localhost:3000 -r 10000 -c 1000 -t 10
  cargo run --release -- -u http://localhost:3000 -r 10000 -c 1000 -m post --header "Content-Type=application/json" --header "Authorization=Bearer token123"
```
