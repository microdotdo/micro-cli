# micro CLI

`micro` pairs with your [micro.do](https://micro.do) account in the browser and deploys small WebAssembly functions.

## Install from source

Install the [Abla compiler](https://github.com/AndreBaltazar8/ablac), then:

```sh
make install
micro login
```

Pairing uses a one-time authorization code, PKCE, and a loopback callback. Access and refresh tokens are written only to the operating system's per-user configuration directory; on Unix, the directory is mode `0700` and the credential file is mode `0600`.

## Deploy

Build one of the functions in [micro-examples](https://github.com/AndreBaltazar8/micro-examples), then run:

```sh
micro deploy hello-micro path/to/function.wasm
micro functions
curl https://hello-micro.micro.do/
```

Function names are DNS labels: 3–63 lowercase letters, digits, or interior hyphens.

## Development

```sh
make test build
```

Set `MICRO_API=http://127.0.0.1:8080` to work against a local server. Plain HTTP is rejected for non-loopback API addresses.

## License

MIT
