# Floccus cli

A Rust-written cli tool for [Floccus](www.floccus.org)

# Status

Early development (use it with care)

# TODO

* [DONE] Read / Print bookmarks sync with git
* [WIP] Add a new bookmark

# Howto use it

* cargo build
* ./target/debug/floccus_cli --help

Print bookmarks:

* Init floccus_cli:
* ./target/debug/floccus_cli -r https://github.com/your_username/your_repo.git print
* After:
* ./target/debug/floccus-cli print

Add a new bookmark (WIP):
* ./target/debug/floccus-cli add -b https://example.com -t "Example www site"

