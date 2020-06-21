use crate::loader::{mods_list, Mod};
use cursive::{
    traits::{Finder, Nameable, Resizable, Scrollable},
    view::ViewWrapper,
    views::{Dialog, LinearLayout, Panel, SelectView},
    Cursive, Vec2, View,
};
use log::*;

struct Half<V: View>(V);

impl<V: View> ViewWrapper for Half<V> {
    type V = V;
    fn with_view<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&Self::V) -> R,
    {
        Some(f(&self.0))
    }
    fn with_view_mut<F, R>(&mut self, f: F) -> Option<R>
    where
        F: FnOnce(&mut Self::V) -> R,
    {
        Some(f(&mut self.0))
    }
    fn wrap_required_size(&mut self, req: Vec2) -> Vec2 {
        debug!(
            "Half-width view asked for required size with constraints {:?}",
            req
        );
        (req.x / 2, req.y).into()
    }
}

pub fn render_lists(cursive: &mut Cursive) {
    let mut available = SelectView::new()
        .with_all(mods_list(cursive).iter().cloned().map(|the_mod| {
            info!(
                "Adding mod {} (dir {}) to \"available\" list",
                the_mod.name(),
                the_mod.path.to_string_lossy()
            );
            (the_mod.name().to_owned(), the_mod)
        }))
        .on_submit(do_select)
        .with_name("Available")
        .scrollable();
    available.get_inner_mut().get_mut().sort_by_label();
    let selected = SelectView::<Mod>::new()
        .on_submit(do_deselect)
        .with_name("Selected")
        .scrollable();

    debug!("Rendering lists of available and selected mods for the first time");
    crate::screen(
        cursive,
        Dialog::new()
            .title("Select mods from the list to be bundled")
            .content(
                LinearLayout::horizontal()
                    .child(Half(Panel::new(available).title("Available")))
                    .child(Half(Panel::new(selected).title("Selected"))),
            )
            .button("Make bundle!", crate::bundler::bundle)
            .h_align(cursive::align::HAlign::Center)
            .with_name("Mods selection")
            .full_screen(),
    );
}

fn do_select(cursive: &mut Cursive, item: &Mod) {
    info!("Selecting mod: {}", item.name());
    if let Some(the_mod) = mods_list(cursive)
        .iter_mut()
        .find(|the_mod| the_mod.path == item.path)
    {
        the_mod.selected = true;
    } else {
        warn!(
            "Attempted to select mod {}, but it wasn't found in loaded list",
            item.name()
        );
    }

    let cb = cursive.call_on_name("Mods selection", |dialog: &mut Dialog| {
        let cb = dialog.call_on_name("Available", |list: &mut SelectView<Mod>| {
            let idx = list
                .iter()
                .position(|(_, the_mod)| the_mod.path == item.path);
            idx.map(|idx| {
                let cb = list.remove_item(idx);
                if idx > 0 {
                    list.select_down(1);
                };
                cb
            })
        });
        dialog.call_on_name("Selected", |list: &mut SelectView<Mod>| {
            list.add_item(item.name(), item.clone());
        });
        cb
    });
    // it's ugly, yeah
    // there are three layers of Options - one from `position` and two from `call_by_name`,
    // and attempt to use `and_then` would be even more ugly
    if let Some(Some(Some(cb))) = cb {
        cb(cursive);
    } else {
        warn!("Failed to select mod - something went wrong!");
    }
}

fn do_deselect(cursive: &mut Cursive, item: &Mod) {
    info!("Deselecting mod: {}", item.name());
    if let Some(the_mod) = mods_list(cursive)
        .iter_mut()
        .find(|the_mod| the_mod.path == item.path)
    {
        the_mod.selected = false;
    } else {
        warn!(
            "Attempted to select mod {}, but it wasn't found in loaded list",
            item.name()
        );
    }

    let cb = cursive.call_on_name("Mods selection", |dialog: &mut Dialog| {
        dialog.call_on_name("Available", |list: &mut SelectView<Mod>| {
            list.add_item(item.name(), item.clone());
            list.sort_by_label();
        });
        dialog.call_on_name("Selected", |list: &mut SelectView<Mod>| {
            let idx = list
                .iter()
                .position(|(_, the_mod)| the_mod.path == item.path);
            idx.map(|idx| {
                let cb = list.remove_item(idx);
                if idx > 0 {
                    list.select_down(1);
                };
                cb
            })
        })
    });
    // it's ugly, yeah
    // there are three layers of Options - one from `position` and two from `call_by_name`,
    // and attempt to use `and_then` would be even more ugly
    if let Some(Some(Some(cb))) = cb {
        cb(cursive);
    } else {
        warn!("Failed to deselect mod - something went wrong!");
    }
}
