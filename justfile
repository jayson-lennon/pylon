clean:
  rm -rf test/public

build: clean
  cargo build

coverage:
  cargo tarpaulin --all --out Html

test:
  cargo nextest run --all
