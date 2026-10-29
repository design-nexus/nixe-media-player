//! Folders: the library as it's laid out on disk, browsed from All videos (its
//! Folders view): a folder shows its folders and the videos directly in it.

use crate::library::{Video, store};
use crate::videotable::{Col, Options, VideoTable};
use crate::widgets;
use crate::{fmt, paths, views};
use gtk::prelude::*;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub struct Node {
    pub path: PathBuf,
    pub name: String,
    pub children: Vec<Rc<Node>>,
    /// Videos in this folder and below.
    pub count: usize,
}

/// Build the folder tree under each library root from the videos' paths.
pub fn tree(roots: &[PathBuf], videos: &[Rc<Video>]) -> Vec<Rc<Node>> {
    fn build(dir: &Path, name: String, counts: &BTreeMap<PathBuf, usize>) -> Node {
        // Direct subfolders that hold videos somewhere below.
        let mut subs: BTreeMap<PathBuf, ()> = BTreeMap::new();
        for d in counts.keys() {
            if let Ok(rest) = d.strip_prefix(dir)
                && let Some(first) = rest.components().next()
            {
                subs.insert(dir.join(first), ());
            }
        }
        let count = counts.iter().filter(|(d, _)| d.starts_with(dir)).map(|(_, n)| n).sum();
        let mut children: Vec<Rc<Node>> = subs
            .into_keys()
            .map(|p| {
                let n = p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                Rc::new(build(&p, n, counts))
            })
            .collect();
        children.sort_by_key(|c| c.name.to_lowercase());
        Node { path: dir.to_path_buf(), name, children, count }
    }
    let mut counts: BTreeMap<PathBuf, usize> = BTreeMap::new();
    for t in videos {
        if let Some(d) = t.path.parent() {
            *counts.entry(d.to_path_buf()).or_default() += 1;
        }
    }
    roots.iter().map(|r| Rc::new(build(r, paths::pretty(r), &counts))).filter(|n| n.count > 0).collect()
}

/// The folders view of All videos: a breadcrumb, the folders here as rows, and
/// the videos directly in this folder with Play and Shuffle. It starts at the
/// library's folders.
pub struct Browser {
    pub root: gtk::Box,
    crumbs: gtk::Box,
    rows: gtk::Box,
    rows_scroll: gtk::ScrolledWindow,
    here: gtk::Box,
    table: VideoTable,
    /// The folder shown; `None` is the library's folders.
    current: RefCell<Option<PathBuf>>,
    /// The videos directly in it, for Play and Shuffle.
    open: Rc<RefCell<Vec<PathBuf>>>,
}

impl Browser {
    pub fn new() -> Rc<Browser> {
        let root = widgets::vbox(0);
        root.set_vexpand(true);
        let crumbs = widgets::hbox(2);
        crumbs.add_css_class("folder-crumbs");
        let rows = widgets::vbox(2);
        rows.add_css_class("folder-list");
        let rows_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::External)
            .propagate_natural_height(true)
            .max_content_height(340)
            .child(&rows)
            .build();
        let here = widgets::hbox(10);
        here.add_css_class("folder-here");
        let here_title = widgets::label("In this folder", "folder-here-title");
        here_title.set_hexpand(true);
        here_title.set_xalign(0.0);
        here.append(&here_title);
        let open: Rc<RefCell<Vec<PathBuf>>> = Rc::default();
        let o = open.clone();
        let actions = views::play_icons(move || o.borrow().clone());
        actions.set_spacing(2);
        here.append(&actions);
        let table = VideoTable::new(Options {
            cols: &[Col::Indicator, Col::Title, Col::Time, Col::Status],
            sortable: false,
            positions: true,
            ..Default::default()
        });
        root.append(&crumbs);
        root.append(&rows_scroll);
        root.append(&here);
        root.append(&table.root);
        // Developer aid, like NMP_SNAPSHOT: NMP_FOLDER=<path> opens the view at that folder.
        let start = std::env::var_os("NMP_FOLDER").map(PathBuf::from);
        let b = Rc::new(Browser { root, crumbs, rows, rows_scroll, here, table, current: RefCell::new(start), open });
        let weak = Rc::downgrade(&b);
        store::subscribe(&b.root, move |c| {
            if c == store::Change::Library
                && let Some(b) = weak.upgrade()
            {
                b.show();
            }
        });
        b.show();
        b
    }

    fn go(self: &Rc<Self>, dir: Option<PathBuf>) {
        *self.current.borrow_mut() = dir;
        self.show();
    }

    /// Draw the folder being shown.
    pub fn show(self: &Rc<Self>) {
        let videos = store::videos();
        let mut nodes = tree(&store::roots(), &videos);
        // With one library folder the top is that folder's contents, not a lone row.
        let single = (nodes.len() == 1).then(|| nodes[0].clone());
        if let Some(only) = &single {
            nodes = only.children.clone();
        }
        let current = self.current.borrow().clone();
        let chain = current.as_deref().map(|dir| chain_to(&nodes, dir)).unwrap_or_default();
        // Gone after a rescan: back to the top (not while the library is still loading).
        if current.is_some() && chain.is_empty() && store::loaded() {
            *self.current.borrow_mut() = None;
            return self.show();
        }
        // The breadcrumb: All folders, then each folder down to this one.
        clear(&self.crumbs);
        let crumb = |text: &str, target: Option<PathBuf>, last: bool| -> gtk::Widget {
            if last {
                return widgets::label(text, "folder-crumb-here").upcast();
            }
            let b = gtk::Button::with_label(text);
            b.add_css_class("flat");
            b.add_css_class("folder-crumb");
            let me = self.clone();
            b.connect_clicked(move |_| me.go(target.clone()));
            b.upcast()
        };
        self.crumbs.append(&crumb("All folders", None, chain.is_empty()));
        for (i, node) in chain.iter().enumerate() {
            let sep = gtk::Image::from_icon_name("go-next-symbolic");
            sep.add_css_class("folder-crumb-sep");
            self.crumbs.append(&sep);
            self.crumbs.append(&crumb(&node.name, Some(node.path.clone()), i + 1 == chain.len()));
        }
        // The folders here: the library's folders at the top, else this one's.
        clear(&self.rows);
        let children = chain.last().map(|n| n.children.clone()).unwrap_or(nodes);
        self.rows_scroll.set_visible(!children.is_empty());
        for node in children {
            let secs: f64 = videos.iter().filter(|v| v.path.starts_with(&node.path)).map(|v| v.duration).sum();
            self.rows.append(&self.folder_row(&node, secs));
        }
        // The videos directly in this folder (at the top: in the one library folder).
        let dir = current.clone().or_else(|| single.map(|n| n.path.clone()));
        let mut here: Vec<Rc<Video>> = match &dir {
            Some(dir) => videos.iter().filter(|v| v.path.parent() == Some(dir.as_path())).cloned().collect(),
            None => Vec::new(),
        };
        here.sort_by(|a, b| a.path.cmp(&b.path));
        *self.open.borrow_mut() = here.iter().map(|v| v.path.clone()).collect();
        self.here.set_visible(!here.is_empty());
        self.table.root.set_visible(!here.is_empty());
        self.table.set(&here);
    }

    fn folder_row(self: &Rc<Self>, node: &Rc<Node>, secs: f64) -> gtk::Button {
        let button = gtk::Button::new();
        button.add_css_class("flat");
        button.add_css_class("folder-entry");
        let line = widgets::hbox(12);
        // A square tile in the accent's tint with the folder glyph in its middle.
        let icon = widgets::hbox(0);
        icon.add_css_class("folder-icon");
        icon.set_size_request(34, 34);
        icon.set_hexpand(false);
        icon.set_valign(gtk::Align::Center);
        let image = gtk::Image::from_icon_name("folder-symbolic");
        image.set_halign(gtk::Align::Center);
        image.set_valign(gtk::Align::Center);
        image.set_hexpand(true);
        icon.append(&image);
        line.append(&icon);
        let text = widgets::vbox(2);
        text.set_hexpand(true);
        text.set_valign(gtk::Align::Center);
        let name = widgets::label(&node.name, "folder-name");
        name.set_xalign(0.0);
        name.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        text.append(&name);
        let mut sub = fmt::count(node.count, "video", "videos");
        if secs > 0.0 {
            sub.push_str(" · ");
            sub.push_str(&fmt::total(secs));
        }
        let sub = widgets::label(&sub, "folder-sub");
        sub.set_xalign(0.0);
        text.append(&sub);
        line.append(&text);
        let chevron = gtk::Image::from_icon_name("go-next-symbolic");
        chevron.add_css_class("folder-chevron");
        line.append(&chevron);
        button.set_child(Some(&line));
        button.set_tooltip_text(Some(&paths::pretty(&node.path)));
        let (me, path) = (self.clone(), node.path.clone());
        button.connect_clicked(move |_| me.go(Some(path.clone())));
        button
    }
}

/// The folders from a library root down to `dir`, or nothing when it isn't there.
fn chain_to(nodes: &[Rc<Node>], dir: &Path) -> Vec<Rc<Node>> {
    for n in nodes {
        if n.path == dir {
            return vec![n.clone()];
        }
        if dir.starts_with(&n.path) {
            let mut rest = chain_to(&n.children, dir);
            if !rest.is_empty() {
                rest.insert(0, n.clone());
                return rest;
            }
        }
    }
    Vec::new()
}

fn clear(b: &gtk::Box) {
    while let Some(c) = b.first_child() {
        b.remove(&c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_tree_with_counts() {
        let t = |p: &str| Rc::new(Video { path: p.into(), ..Default::default() });
        let tracks = vec![t("/m/A/x/1.mkv"), t("/m/A/x/2.mkv"), t("/m/A/3.mkv"), t("/m/B/4.mkv")];
        let roots = tree(&[PathBuf::from("/m"), PathBuf::from("/empty")], &tracks);
        assert_eq!(roots.len(), 1);
        let m = &roots[0];
        assert_eq!(m.count, 4);
        assert_eq!(m.children.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["A", "B"]);
        assert_eq!(m.children[0].count, 3);
        assert_eq!(m.children[0].children[0].name, "x");
        assert_eq!(m.children[0].children[0].count, 2);
    }
}
