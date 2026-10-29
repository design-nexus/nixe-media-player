//! All videos: one sortable table of everything, filtered by kind, or the same
//! library by folder (the Folders view, see `folders::Browser`).

use super::{library_stack, scan_banner};
use crate::fmt;
use crate::library::{Kind, store};
use crate::videotable::{Options, VideoTable};
use crate::widgets::{self, Page};
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

thread_local! {
    static FILTER: RefCell<String> = RefCell::new("all".into());
    /// "videos" or "folders"; the page's switch between them.
    static VIEW: RefCell<String> = RefCell::new("videos".into());
    /// The page's Videos/Folders switch and the folder browser, once built.
    static VIEWS: RefCell<Option<(gtk::Box, Rc<super::folders::Browser>)>> = const { RefCell::new(None) };
}

/// Show All videos by folder (what the Folders page used to be).
pub fn want_folders() {
    VIEW.with(|v| *v.borrow_mut() = "folders".into());
    let switch = VIEWS.with(|v| v.borrow().as_ref().map(|(s, _)| s.clone()));
    // The switch's second segment; turning it on shows the folders.
    if let Some(folders) = switch.and_then(|s| s.last_child()).and_downcast::<gtk::ToggleButton>() {
        folders.set_active(true);
    }
}

pub fn build(page: &Page) {
    page.body.append(&scan_banner());
    let content = widgets::vbox(0);
    // The page's head: its name and how many, the kind filter (Videos view only)
    // and the switch between the table and the folders.
    let toolbar = widgets::hbox(10);
    toolbar.add_css_class("toolbar");
    toolbar.add_css_class("page-head");
    toolbar.append(&widgets::label("All videos", "page-head-title"));
    let count = widgets::label("", "page-head-count");
    count.set_hexpand(true);
    count.set_xalign(0.0);
    toolbar.append(&count);
    let table = VideoTable::new(Options::default());
    let refresh = {
        let (table, count) = (table.clone(), count.clone());
        Rc::new(move || {
            let filter = FILTER.with(|f| f.borrow().clone());
            let all = store::videos();
            let shown: Vec<_> = all
                .iter()
                .filter(|v| match filter.as_str() {
                    "movies" => v.kind == Kind::Movie,
                    "episodes" => v.kind == Kind::Episode,
                    "other" => v.kind == Kind::Other,
                    _ => true,
                })
                .cloned()
                .collect();
            count.set_text(&fmt::thousands(shown.len()));
            table.set(&shown);
        })
    };
    let r = refresh.clone();
    let kinds = widgets::segmented(
        &widgets::opts(&[("all", "All"), ("movies", "Movies"), ("episodes", "Episodes"), ("other", "Other")]),
        "all",
        move |id| {
            FILTER.with(|f| *f.borrow_mut() = id);
            r();
        },
    );
    toolbar.append(&kinds);
    let views = gtk::Stack::new();
    views.set_vexpand(true);
    views.add_named(&table.root, Some("videos"));
    let browser = super::folders::Browser::new();
    views.add_named(&browser.root, Some("folders"));
    let view = VIEW.with(|v| v.borrow().clone());
    let (vs, ks) = (views.clone(), kinds.clone());
    let switch = widgets::segmented(&widgets::opts(&[("videos", "Videos"), ("folders", "Folders")]), &view, move |id| {
        vs.set_visible_child_name(&id);
        ks.set_visible(id == "videos");
        VIEW.with(|v| *v.borrow_mut() = id);
    });
    toolbar.append(&switch);
    views.set_visible_child_name(&view);
    kinds.set_visible(view == "videos");
    VIEWS.with(|v| *v.borrow_mut() = Some((switch.clone(), browser)));
    content.append(&toolbar);
    content.append(&views);
    let empty = widgets::vbox(0);
    page.body.append(&library_stack(&content, || true, empty));
    refresh();
    store::subscribe(&content, move |c| {
        if c == store::Change::Library {
            refresh();
        }
    });
}
