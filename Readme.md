# Floccus cli

A cli tool (written in [Rust](https://www.rust-lang.org)) for [Floccus](www.floccus.org)

Limitations:
* Sync backend: git ([create an issue for other backend](https://github.com/sydhds/floccus_cli/issues/new/choose))

## Install

cargo install --locked --git https://github.com/sydhds/floccus_cli.git

## Uninstall

cargo uninstall floccus-cli

## Quickstart

- Setup Floccus and setup sync with a git repository
- Init floccus-cli config file:
  - Using git https url + token (with write access): 
    - floccus-cli -g https://github.com/_USERNAME_/_REPO_NAME_.git -t __GITHUB__TOKEN_ init
  - Using git ssh url:
    - floccus-cli -g ssh://git@github.com/_USERNAME_/_REPO_NAME_.git init
- floccus-cli print
- floccus-cli add -b https://example.com -t "Example www site" -u after=3

### Add 

* Add a bookmark after a given id (folder or bookmark)
  * floccus-cli add -b https://example.com -t "Example www site" -u after=3 --disable-push
* Add a bookmark in a given folder id (append)
  * floccus-cli add -b https://example.com -t "Example www site" -u 2 --disable-push
  * floccus-cli add -b https://example.com -t "Example www site" -u append=2 --disable-push
* Add a bookmark in a given folder id (prepend)
  * floccus-cli add -b https://example.com -t "Example www site" -u prepend=2 --disable-push

### Rm

* Remove a bookmark using a given id
  * floccus-cli rm -i 14 --disable-push

### Find

* floccus-cli find "FOO"
* floccus-cli find --bookmark "FOO"
* floccus-cli find --bookmark --title "FOO"

### Misc

* Verbose mode: RUST_LOG=debug floccus-cli print

## Contrib

All contributions, code, feedback and strategic advice, are welcome. If you have a question you can open an issue on the repository. 

### For Dev

* cargo doc --lib -p floccus-xbel

## License

MPL-2.0 (see [LICENSE](./LICENSE))