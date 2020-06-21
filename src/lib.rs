mod bundler;
mod loader;
mod paths;
mod select;

use cursive::{
    event::{Event, Key},
    traits::{Nameable, Resizable},
    views::{Dialog, EditView, PaddedView, TextView},
    Cursive, View,
};
use log::*;
use std::error::Error;

fn push_screen<T: cursive::View>(cursive: &mut Cursive, view: T) {
    cursive.add_layer(PaddedView::lrtb(1, 1, 1, 1, view).max_width(cursive.screen_size().x - 10));
}
fn screen<T: cursive::View>(cursive: &mut Cursive, view: T) {
    cursive.pop_layer();
    push_screen(cursive, view);
}
fn error(cursive: &mut Cursive, mut err: &(dyn Error + 'static)) {
    let desc = err.to_string();
    error!("Error encountered: {}", desc);
    while let Some(source) = err.source() {
        info!("Caused by:\n  {}", source);
        err = source;
    }
    screen(
        cursive,
        Dialog::around(TextView::new(desc))
            .button("OK", |cursive| cursive.quit())
            .title("Error"),
    );
}

fn run_update<F: FnOnce(&mut Cursive) + 'static + Send>(sink: &mut cursive::CbSink, cb: F) {
    sink.send(Box::new(cb))
        .expect("Cursive sink was unexpectedly dropped, this is probably a bug");
}

pub fn run() {
    let mut cursive: Cursive = cursive::default();

    info!("Creating initial dialog");
    let dialog = cursive::views::Dialog::new()
        .content(
            EditView::new()
                .on_submit_mut(loader::load_path)
                .with_name("Library path")
                .full_width(),
        )
        .title("Steam library path:")
        .button("List mods", |cursive| {
            info!("List mods button click");
            cursive.call_on_name("Library path", |view: &mut EditView| {
                view.on_event(Event::Key(Key::Enter))
            });
        })
        .full_width();
    screen(&mut cursive, dialog);

    info!("Starting Cursive");
    cursive.run();
}
