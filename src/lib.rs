mod bundler;
mod paths;
mod select;

use cursive::{
    event::{Event, Key},
    traits::{Nameable, Resizable},
    views::{EditView, PaddedView},
    Cursive, View,
};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
struct Project {
    #[serde(rename = "Title")]
    title: String,
}

#[derive(Default, Debug, Clone)]
struct Mod {
    selected: bool,
    name: String,
    path: PathBuf,
}

struct Data {
    base_path: PathBuf,
    mods: Vec<Mod>,
}

fn mods_list(cursive: &mut Cursive) -> &mut [Mod] {
    &mut cursive
        .user_data::<crate::Data>()
        .expect("Mods data wasn't set")
        .mods
}

fn load_path(cursive: &mut Cursive, base_path: &str) {
    let base_path = base_path.into();
    let path = paths::workshop(&base_path);
    let mods = std::fs::read_dir(path)
        .expect("Failed to read dir")
        .map(|item| {
            item.map_err(|err| Box::new(err) as Box<dyn std::error::Error>)
                .and_then(|entry| {
                    let path = entry.path();
                    let file = std::fs::File::open(path.join("project.xml"))?;
                    let project: Project = serde_xml_rs::from_reader(file)?;
                    Ok(Mod {
                        selected: false,
                        name: project.title,
                        path,
                    })
                })
        })
        .collect::<Result<Vec<_>, _>>()
        .expect("Error iterating");
    cursive.set_user_data(Data { base_path, mods });
    select::render_lists(cursive);
}

fn screen<T: cursive::View>(cursive: &mut Cursive, view: T) {
    cursive.pop_layer();
    cursive.add_layer(PaddedView::lrtb(10, 10, 10, 10, view).full_screen());
}

pub fn run() {
    let mut cursive: Cursive = cursive::default();

    let dialog = cursive::views::Dialog::new()
        .content(
            EditView::new()
                .on_submit_mut(load_path)
                .with_name("Library path")
                .full_width(),
        )
        .title("Steam library path:")
        .button("List mods", |cursive| {
            cursive.call_on_name("Library path", |view: &mut EditView| {
                view.on_event(Event::Key(Key::Enter))
            });
        })
        .full_width();

    screen(&mut cursive, dialog);

    cursive.run();
}
