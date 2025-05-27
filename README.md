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
  -b, --body <BODY>                Request body as string [default: empty]
  -H, --header <KEY=VALUE>         Custom HTTP header [allows multiple]
  -h, --help                       Print help
  -V, --version                    Print version
```

## Examples

```sh
  cargo run --release -- -u http://localhost:3000 -r 1000
  cargo run --release -- -u http://localhost:3000 -r 10000 -c 1000 -t 10
  cargo run --release -- -u http://localhost:3000 -r 10000 -c 1000 -m post --body '{"data": "foo"}'
  cargo run --release -- -u http://localhost:3000 -r 10000 -c 1000 -m post --header "Content-Type=application/json" --header "Authorization=Bearer token123"
```

[![benchpress.png](https://i.postimg.cc/7Lj1H5nG/benchpress.png)](https://postimg.cc/qtX6QMY4)
