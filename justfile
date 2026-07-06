alias b := build
alias r := run

default: check build test

# installs necessary components, prerequisite is having rustup installed
[group('rust')]
setup:
    rustup update
    rustup component add rustfmt clippy

# run cargo fmt and clippy lints
[group('rust')]
[group('lint')]
check:
    cargo fmt --check
    cargo clippy -- -D warnings

# build the application
[group('rust')]
build target="":
    cargo build --{{target}}

# run the application
[group('rust')]
run target="":
    cargo run --{{target}}

# runs project unit tests
[group('rust')]
test:
    cargo test --all-targets

# shows docs for the project in the browser
[group('rust')]
docs:
    cargo doc --no-deps --open

# cleans the cargo build cache
[group('rust')]
clean:
    cargo clean
    rm -rf target/
