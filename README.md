# micro CLI

`micro` builds, previews, deploys, and maintains full-stack WebAssembly sites on [micro.do](https://micro.do).

## Install from source

Install the [Abla compiler](https://github.com/AndreBaltazar8/ablac), then:

```sh
make install
micro login
```

Pairing uses a one-time authorization code, PKCE, and a loopback callback. Access and refresh tokens are written only to the operating system's per-user configuration directory; on Unix, the directory is mode `0700` and the credential file is mode `0600`.

## Create and develop

```sh
micro new my-site
cd my-site
micro dev
```

`micro dev` builds the exact production bundle and starts the official runner
on `http://127.0.0.1:8787`. Its app users, data, purchases, entitlements, and
purchase events are disposable in-memory fixtures. Override the runner binary
with `MICRO_RUNNER` and the compiler with `MICRO_ABLAC` when developing those
repositories together.

## Deploy and maintain

```sh
micro signup --email you@example.com
micro deploy my-site
micro status
micro logs --since 30m
micro pull my-site another-checkout
```

The first successful deployment atomically claims an available slug. Subsequent
deployments use the stored source revision and reject stale updates instead of
silently overwriting another checkout. Optional `micro.yaml` products are
synchronized non-destructively; protected files are uploaded explicitly with
`micro files upload`.

## Development

```sh
make test build
```

Set `MICRO_API=http://127.0.0.1:8080` to work against a local control plane.
Plain HTTP is rejected for non-loopback API addresses.

## License

MIT
