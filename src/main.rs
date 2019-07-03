#[macro_use]
extern crate error_chain;
#[macro_use]
extern crate clap;
extern crate serde_json;

mod errors {
	error_chain! { }
}

mod format;
mod disptree;
mod keybinder;
mod curses;
mod jsonvalue;

use errors::*;
use disptree::*;
use jsonvalue::*;

/*
 * TODO:
 *     Search
 *     Cut down on excess redrawing
 *     Delay in mouse events
 *     TODOs, FIXMEs, and `unwrap()`s
 * Future:
 *     Pluggable backends https://michael-f-bryan.github.io/rust-ffi-guide/dynamic_loading.html https://github.com/Zaerei/rust_plugin_playground
 *         Reddit, Hacker News https://hacker-news.firebaseio.com/v0/item/18918215.json https://hacker-news.firebaseio.com/v0/topstories.json
 *     Customize colors and key bindings
 *     jq integration: https://crates.io/crates/json-query
 * Ideas:
 *     ncurses replacement: https://github.com/TimonPost/crossterm https://github.com/redox-os/termion
 * Bugs:
 *     Serde doesn't give us object elements in document order.  Is there any way to achieve this?
 */

fn run() -> Result<()> {
	let args = clap_app!(jsonb =>
		(version: crate_version!())
		(about: "Curses-style JSON viewer and editor")
		(@arg file:  index(1) "The file to read")
	).get_matches();

	let json: Box<JsonSource> = if let Some(fname) = args.value_of("file") {
		JsonSource::read(std::io::BufReader::new(std::fs::File::open(fname).chain_err(|| "could not open file")?))
	}
	else {
		let stdin = std::io::stdin();
		let inlock = stdin.lock();
		JsonSource::read(inlock)
	}.chain_err(|| "failed to load input data")?;
	curses::setup();
	let mut dt = DispTree::new(Box::new(json.root()), json.colors());
	dt.interactive();
	curses::cleanup();
	Ok(())
}

quick_main!(run);
