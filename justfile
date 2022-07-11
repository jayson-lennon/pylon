clean:
  rm -rf test/public

build: clean
  cargo build

coverage:
  cargo tarpaulin --all --out Html

test:
  cargo nextest run --all

aur:
  rm -rf pkg
  mkdir -p pkg
  cargo aur
  mv PKGBUILD pkg
  mv pylon-*.gz pkg
  cd pkg && makepkg