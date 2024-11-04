# Floccus cli

A cli tool (written in [Rust](https://www.rust-lang.org)) for [Floccus](www.floccus.org)

Limitations:
* Sync is only git ([create an issue for other backend](https://github.com/sydhds/floccus_cli/issues/new/choose))
* [Git ssh url not supported](https://github.com/sydhds/floccus_cli/issues/5)

## Install

cargo install --locked --git https://github.com/sydhds/floccus_cli.git

## Quickstart

- Setup Floccus and setup sync with a git repository
- Init floccus-cli config file:
  - floccus-cli -g https://__GITHUB_TOKEN__@github.com/your_username/your_repo.git print
- floccus-cli print
- floccus-cli add -b https://example.com -t "Example www site" -u after=3 --disable-push

## Documentation

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

## Contrib

All contributions, code, feedback and strategic advice, are welcome. If you have a question you can open an issue on the repository. 

## License

MPL-2.0 (see [LICENSE](./LICENSE))