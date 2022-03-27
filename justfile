clean:
  rm -rf test/public

build: clean
  cargo run -- build

serve: clean
  cargo run -- serve
