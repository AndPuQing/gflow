// Copy from https://docs.rs/crate/ascii_tree/latest/source/src/lib.rs
//! Crate to write an ascii tree.
//!
//! ```rust
//! let l1 = Leaf(vec![String::from("line1"), String::from("line2"), String::from("line3"), String::from("line4")]);
//! let l2 = Leaf(vec![String::from("only one line")]);
//! let n1 = Node(String::from("node 1"), vec![l1.clone(), l2.clone()]);
//! let n2 = Node(String::from("node 2"), vec![l2.clone(), l1.clone(), l2.clone()]);
//! let n3 = Node(String::from("node 3"), vec![n1.clone(), l1.clone(), l2.clone()]);
//! let n4 = Node(String::from("node 4"), vec![n1, n2, n3]);
//!
//! let mut output = String::new();
//! let _ = write_tree(&mut output, &n4);
//! ```
//!
//! The result would be:
//! <pre>
//! node 4
//! ├─ node 1
//! │  ├─ line1
//! │  │  line2
//! │  │  line3
//! │  │  line4
//! │  └─ only one line
//! ├─ node 2
//! │  ├─ only one line
//! │  ├─ line1
//! │  │  line2
//! │  │  line3
//! │  │  line4
//! │  └─ only one line
//! └─ node 3
//!    ├─ node 1
//!    │  ├─ line1
//!    │  │  line2
//!    │  │  line3
//!    │  │  line4
//!    │  └─ only one line
//!    ├─ line1
//!    │  line2
//!    │  line3
//!    │  line4
//!    └─ only one line
//! </pre>

use std::fmt;
use std::fmt::Write;

#[derive(Clone)]
pub enum Tree {
    Node(String, Vec<Tree>),
    Leaf(Vec<String>),
}

#[inline]
/// writes a tree in an ascii tree to the writer
///
/// ```
/// let mut output = String::new();
/// write_tree(&mut output, &tree);
///
/// ```
pub fn write_tree<T: Write>(f: &mut T, tree: &Tree) -> fmt::Result {
    write_tree_element(f, tree, &vec![])
}

fn write_tree_element<T: Write>(f: &mut T, tree: &Tree, level: &Vec<usize>) -> fmt::Result {
    use Tree::*;
    const EMPTY: &str = "   ";
    const EDGE: &str = " └─";
    const PIPE: &str = " │ ";
    const BRANCH: &str = " ├─";

    let maxpos = level.len();
    let mut second_line = String::new();
    for (pos, l) in level.iter().enumerate() {
        let last_row = pos == maxpos - 1;
        if *l == 1 {
            if !last_row {
                write!(f, "{}", EMPTY)?
            } else {
                write!(f, "{}", EDGE)?
            }
            second_line.push_str(EMPTY);
        } else {
            if !last_row {
                write!(f, "{}", PIPE)?
            } else {
                write!(f, "{}", BRANCH)?
            }
            second_line.push_str(PIPE);
        }
    }
    match tree {
        Node(title, children) => {
            let mut d = children.len();
            write!(f, " {}\n", title)?;
            for s in children {
                let mut lnext = level.clone();
                lnext.push(d);
                d -= 1;
                write_tree_element(f, s, &lnext)?;
            }
        }
        Leaf(lines) => {
            for (i, s) in lines.iter().enumerate() {
                match i {
                    0 => writeln!(f, " {}", s)?,
                    _ => writeln!(f, "{} {}", second_line, s)?,
                }
            }
        }
    }
    Ok(())
}
