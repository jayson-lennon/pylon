clean:
  rm -rf test/public

build: clean
  cargo build

coverage:
  cargo tarpaulin --out Html
