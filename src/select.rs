use crate::{mods_list, Mod};
use cursive::{
    traits::{Finder, Nameable, Resizable, Scrollable},
    view::ViewWrapper,
    views::{Dialog, LinearLayout, Panel, SelectView},
    Cursive, View, Vec2,
};

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
        (req.x / 2, req.y).into()
    }
}

pub fn render_lists(cursive: &mut Cursive) {
    let mut available = SelectView::new()
        .with_all(
            mods_list(cursive)
                .iter()
                .cloned()
                .map(|the_mod| (the_mod.name().to_owned(), the_mod)),
        )
        .on_submit(do_select)
        .with_name("Available")
        .scrollable();
    available
        .get_inner_mut()
        .get_mut()
        .sort_by_label();
    let selected = SelectView::<Mod>::new()
        .on_submit(do_deselect)
        .with_name("Selected")
        .scrollable();

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
    eprintln!("Selecting {:?}", item);
    mods_list(cursive)
        .iter_mut()
        .find(|the_mod| the_mod.path == item.path)
        .map(|the_mod| the_mod.selected = true);
    let cb = cursive.call_on_name("Mods selection", |dialog: &mut Dialog| {
        let cb = dialog.call_on_name("Available", |list: &mut SelectView<Mod>| {
            let idx = list
                .iter()
                .position(|(_, the_mod)| the_mod.path == item.path);
            let cb = idx.map(|idx| list.remove_item(idx));
            list.select_down(1);
            cb
        });
        dialog.call_on_name("Selected", |list: &mut SelectView<Mod>| {
            list.add_item(item.name(), item.clone());
        });
        cb
    });
    cb.map(|cb| {
        cb.map(|cb| cb.map(|cb| cb(cursive)));
    });
}

fn do_deselect(cursive: &mut Cursive, item: &Mod) {
    eprintln!("Deselecting {:?}", item);
    mods_list(cursive)
        .iter_mut()
        .find(|the_mod| the_mod.path == item.path)
        .map(|the_mod| the_mod.selected = false);
    let cb = cursive.call_on_name("Mods selection", |dialog: &mut Dialog| {
        dialog.call_on_name("Available", |list: &mut SelectView<Mod>| {
            list.add_item(item.name(), item.clone());
            list.sort_by_label();
        });
        let cb = dialog.call_on_name("Selected", |list: &mut SelectView<Mod>| {
            let idx = list
                .iter()
                .position(|(_, the_mod)| the_mod.path == item.path);
                let cb = idx.map(|idx| list.remove_item(idx));
                list.select_down(1);
                cb
        });
        cb
    });
    cb.map(|cb| {
        cb.map(|cb| cb.map(|cb| cb(cursive)));
    });
}
