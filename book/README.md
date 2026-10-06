# The SLC book

`src/` is a tutorial and a reference for SLC, built with
[mdBook](https://rust-lang.github.io/mdBook/). The language specification
stays in [`DESIGN.md`](../DESIGN.md). This book teaches the surface and lists
what the compiler accepts.

The programs in `examples/` are included into the pages. `check.sh` formats
them, runs each one under `slc run --interpret`, and compares the output with
the saved `.out` file. A file named `*.check.sl` is type-checked and not run.

```sh
cargo build -p slc-driver
./book/check.sh
```

## Build the site

```sh
cargo install mdbook --version 0.4.52 --locked
mdbook build book    # writes book/book/
mdbook serve book    # http://localhost:3000
```

`book/book/` is generated and gitignored.

## GitHub Pages

The workflow [`.github/workflows/pages.yml`](../.github/workflows/pages.yml)
builds the book on every pull request and push. On a push to `master` or
`main` it deploys the site.

Once, in the repository settings:

1. Open **Settings → Pages**.
2. Set **Source** to **GitHub Actions**.

The published URL for this repository is
<https://keens.github.io/slc/>. `book.toml` sets `site-url` to `/slc/` to
match a project site. A repository published at the root of a user site uses
`site-url = "/"`.

Until Pages is enabled, the deploy job has nowhere to publish. The build job
still runs.
