# Floccus cli

A cli tool (written in [Rust](https://www.rust-lang.org)) for [Floccus](www.floccus.org)

Limitations:
* Sync is only git ([create an issue for other backend](https://github.com/sydhds/floccus_cli/issues/new/choose))
* [Git ssh url not supported](https://github.com/sydhds/floccus_cli/issues/5)

## Install

cargo install --locked --git https://github.com/sydhds/floccus_cli.git

## Quickstart

- Setup floccus and sync with a git repository
- Init floccus_cli config file:
  - floccus_cli -g https://__GITHUB_TOKEN__@github.com/your_username/your_repo.git print
- floccus_cli print
- floccus_cli add -b https://example.com -t "Example www site" -u after=3 --disable-push

## Documentation

### Add 

* Add a bookmark after a given id (folder or bookmark)
  * floccus_cli add -b https://example.com -t "Example www site" -u after=3 --disable-push
* Add a bookmark in a given folder id (append)
  * floccus_cli add -b https://example.com -t "Example www site" -u 2 --disable-push
  * floccus_cli add -b https://example.com -t "Example www site" -u append=2 --disable-push
* Add a bookmark in a given folder id (prepend)
  * floccus_cli add -b https://example.com -t "Example www site" -u prepend=2 --disable-push

### Rm

* Remove a bookmark using a given id
  * floccus_cli rm -i 14 --disable-push

### Find

* ./target/debug/floccus-cli find "FOO"
* ./target/debug/floccus-cli find --bookmark "FOO"
* ./target/debug/floccus-cli find --bookmark --title "FOO"

## Contrib

All contributions, code, feedback and strategic advice, are welcome. If you have a question you can open an issue on the repository. 

## License

MPL-2.0 (see [LICENSE](./LICENSE))