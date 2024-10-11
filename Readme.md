# Floccus cli

A Rust-written cli tool for [Floccus](www.floccus.org)

## Status

* [Backend]
  * Git
  * [TODO] Google Drive
  * [TODO] Nextcloud

### Backend Git - Status 

* [DONE] Read / Print bookmarks sync with git
* [EXPERIMENTAL] Add a new bookmark
* [TODO] Rm/Find bookmark
* [TODO] Config file
* [TODO] CI

## Howto

* cargo build
* ./target/debug/floccus_cli --help

Print bookmarks:

* Init floccus_cli:
* ./target/debug/floccus_cli -r https://github.com/your_username/your_repo.git print
* After:
* ./target/debug/floccus-cli print

Add a new bookmark (EXPERIMENTAL, Default: append to root):
* ./target/debug/floccus-cli add -b https://example.com -t "Example www site" --disable-push


* Add a bookmark after a given id (folder or bookmark)
  * ./target/debug/floccus-cli add -b https://example.com -t "Example www site" -u after=3 --disable-push
* Add a bookmark in a given folder id (append)
    * ./target/debug/floccus-cli add -b https://example.com -t "Example www site" -u 2 --disable-push
    * ./target/debug/floccus-cli add -b https://example.com -t "Example www site" -u append=2 --disable-push
* Add a bookmark in a given folder id (prepend)
    * ./target/debug/floccus-cli add -b https://example.com -t "Example www site" -u prepend=2 --disable-push

Note: 
* Pushing (git push) by floccus-cli is experimental - for now use --disable-push && push manually

