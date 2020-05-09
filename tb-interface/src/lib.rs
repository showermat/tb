//! This crate defines the interface for making plugins for TB.
//!
//! The basic procedure for implementing a TB plugin looks like this:
//!
//!  1. Create a struct for the fundamental value of your plugin (a single node in the tree), and
//!     implement `Value` for it.
//!
//!  2. Provide an implementation of `Source` that holds any data that need to exist once per tree
//!     (a file handle, owned tree root, or maybe nothing).
//!
//!  3. Provide an implementation of `Factory` that provides some basic information about your
//!     plugin and can create new sources from command-line arguments.
//!
//!  4. Expose a public `#[no_mangle]` function called `get_factories` in the root of your crate
//!     that returns a `Vec<Box<Factory>>` containing your newly created factory/ies.
//!
//!  5. Compile as a dynamic library with `crate_type = ["cdylib"]` (I feel like `dylib` should be
//!     the right choice, but it doesn't work as well for me).
//!
//!  6. Place the resulting dynamic library in `$HOME/.local/share/tb/plugins` (depending on the
//!     value of `$XDG_DATA_HOME`) and run `tb help` to make sure it's picked up.
//!
//! The `rand` backend (provided as part of tb-sample-plugins) is a good example of about the
//! simplest possible working backend.  It maintains no state, and simply generates a random tree on
//! request.  Check it out for help getting started.  The `fs` and `json` plugins built into TB are
//! good examples of more practical backends.

#[macro_use]
extern crate error_chain;
extern crate enumflags2;
#[macro_use]
extern crate enumflags2_derive;

pub use enumflags2::BitFlags;

pub mod errors {
	error_chain! { }
}

#[derive(EnumFlags, Copy, Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum Render {
	Debug = 0x1,
	Search = 0x2,
	Yank = 0x4,
}

/// Formatting is described by an enum tree that is rendered by TB to the appropriate sequence of
/// escapes.  All formatting functionality is provided by these enums.  If a backend uses
/// formatting commands heavily, consider `use`ing the `fmt` module, which provides slightly
/// shorter abbreviations for these items.
pub enum Format {
	/// A literal string.  All characters drawn on the screen ultimately come from `Literal`s.
	/// Embed a `Literal` inside another format nodes for more interesting effects.
	Literal(String),

	/// A container for concatenating a number of other format nodes.
	Container(Vec<Format>),

	/// Color the enclosed format nodes with the specified color.  The first argument is the index
	/// of a color defined by `Factory::colors`.  All sub-nodes will inherit this color, but it can
	/// be overridden.
	Color(usize, Box<Format>),

	/// Prevent automatic line wrapping in sub-nodes.  If there is a string of characters that need
	/// to stay together, wrap them in a `NoBreak`.  Keep it short, though -- TB does not currently
	/// support `NoBreak`s with lines longer than the screen width.  Hard wraps and line breaks
	/// inside `NoBreak`s are also not supported.  Relaxing these requirements is a high priority.
	NoBreak(Box<Format>),

	/// Exclude sub-nodes from a given type of rendering.  For example, this can be used to exclude
	/// decorative characters from being included in string searches.
	Exclude(BitFlags<Render>, Box<Format>),
}

/// To support both 8-color and 256+-color terminals, every color specification requires a standard
/// ANSI color (0 to 7) and an XTerm color (0 to 255).
#[derive(Clone, Copy)]
pub struct Color {
	pub c8: u8,
	pub c256: u8,
}

/// A single value in the display tree.  This corresponds to a single array, object, or primitive
/// value in JSON, a comment in a thread, a file or directory in a filesystem, or whatever other
/// entity constitutes the nodes of the tree you are modeling.
///
/// Note that `Results` are not accepted as return types.  This is because there is typically no
/// meaningful error handling that TB can do on behalf of the backend.  Either the backend can
/// handle the error internally -- which it should do, either silently or by creating a `Value`
/// node exposing the error message -- or it is a fatal error and the backend should simply panic
/// and the application will clean up and abort.
pub trait Value<'a> {
	/// Returns the format tree representing the content of this node.
	fn content(&self) -> Format;
	
	/// Whether this node is logically expandable.  Note that it is acceptable to have nodes that
	/// are expandable but have no children.  For most purposes, they are treated the same.
	fn expandable(&self) -> bool;

	/// The children of this node.  This is guaranteed not to be called if `expandable` is false.
	fn children(&self) -> Vec<Box<dyn Value<'a> + 'a>>;

	/// If it is desirable to format the value differently when it is collapsed, specify that
	/// format here.  When the value is collapsed, the format returned by `placeholder` will be
	/// used; when it is expanded, the format returned by `content` will be used.  By default, this
	/// just mirrors `content`.
	fn placeholder(&self) -> Format { self.content() }

	/// Define an action to be run when the user "invokes" the value (by default, presses enter
	/// when this node is selected).  This can be used to run some action on the current node --
	/// for example, edit a JSON value, open a URL in a browser, or open a file in its associated
	/// application.
	fn invoke(&self) { }
}

/// An object that is responsible for owning of a value tree.  It can maintain any state necessary
/// for the entire tree, and exists at least as long as any node in the tree.  It is only used on
/// program startup, to retrieve the root of the tree.
pub trait Source {
	/// Return the root of the tree to be displayed.
	fn root<'a>(&'a self) -> Box<dyn Value<'a> + 'a>;
}

pub struct Info {
	pub name: &'static str,
	pub desc: &'static str,
}

/// A factory object provides some basic information about the backend, and is able to create
/// sources on request.
pub trait Factory {
	/// Get some basic human-oriented information about the backend for display in the backend
	/// list.
	fn info(&self) -> Info;

	/// Create a backend based on a sequence of arguments.  The string slice passed in is the
	/// command-line arguments for this invocation (stripped of the binary name and any global
	/// arguments used by TB itself).  The implementer is welcome to interpret these any way it
	/// wishes -- accepting a single URL or file path in simple cases, or doing full command-line
	/// parsing for more complex applications.  Common flags like `--help` are *not* automatically
	/// handled.
	///
	/// If the arguments passed in are valid but do not result in a source being created (for
	/// example, requesting help or version information), return `None` and the application will
	/// simply exit normally.  If there is some problem with the input, return `Some(Err)` and TB
	/// will print an error trace and abort.  Otherwise, return `Some(Ok(Box<Source>))` and TB will
	/// enter interactive mode.
	fn from(&self, &[&str]) -> Option<errors::Result<Box<dyn Source>>>;
	fn colors(&self) -> Vec<Color> { vec![] }
}

/// Formatting shortcuts to make tree-building easier.  You can `use` the `fmt` module, and then
/// construct trees fairly quickly using these abbreviations.
pub mod fmt {
	use super::*;
	pub fn lit(s: &str) -> Format { Format::Literal(s.to_string()) }
	pub fn cat(children: Vec<Format>) -> Format { Format::Container(children) }
	pub fn color(c: usize, child: Format) -> Format { Format::Color(c, Box::new(child)) }
	pub fn nobreak(child: Format) -> Format { Format::NoBreak(Box::new(child)) }
	pub fn exclude(render: BitFlags<Render>, child: Format) -> Format { Format::Exclude(render, Box::new(child)) }
	pub fn nosearch(child: Format) -> Format { Format::Exclude(BitFlags::from(Render::Search), Box::new(child)) }
	pub fn noyank(child: Format) -> Format { Format::Exclude(BitFlags::from(Render::Yank), Box::new(child)) }
	pub fn hide(child: Format) -> Format { Format::Exclude(Render::Search | Render::Yank, Box::new(child)) }
}
