<p align="center">
  <a href="https://sentry.io/?utm_source=github&utm_medium=logo" target="_blank">
    <picture>
      <source srcset="https://sentry-brand.storage.googleapis.com/sentry-logo-white.png" media="(prefers-color-scheme: dark)" />
      <source srcset="https://sentry-brand.storage.googleapis.com/sentry-logo-black.png" media="(prefers-color-scheme: light), (prefers-color-scheme: no-preference)" />
      <img src="https://sentry-brand.storage.googleapis.com/sentry-logo-black.png" alt="Sentry" width="280">
    </picture>
  </a>
</p>

# Sentry vroomrs

[![GitHub Release](https://img.shields.io/github/release/getsentry/vroomrs.svg)](https://github.com/getsentry/vroomrs/releases/latest)

`vroomrs` is Sentry's profiling library (a Python extension module), processing and deriving data about your profiles. It's written in Rust.

The name was inspired by this [video](https://www.youtube.com/watch?v=t_rzYnXEQlE).

## Development

In order to develop for `vroomrs`, you will need:
- `rust`
- `python`
- `make`
- `pre-commit`

## pre-commit

In order to install `pre-commit`, you will need `python` and run:
```sh
pip install --user pre-commit
```

Once `pre-commit` is installed, you'll have to set up the actual git hook scripts with:
```sh
pre-commit install
```

## Build

```sh
make build
```

## Docs

After a successful build, the module api documentation can be found under `docs > build > html > index.html`

## Run tests

```sh
make test
```