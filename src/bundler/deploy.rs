use super::{
    error::DeploymentError,
    game_data::{DeployableStructured, GameData, GameDataItem},
};
use crossbeam_channel::{bounded, Sender};
use cursive::{
    traits::{Nameable, Resizable},
    views::{Dialog, EditView, LinearLayout, Panel, TextView},
    Cursive,
};
use log::*;
use std::path::Path;

#[derive(Copy, Clone)]
enum OverwriteChoice {
    Overwrite,
    Retry,
    Cancel,
}

pub fn deploy(
    sink: &mut cursive::CbSink,
    mod_path: &Path,
    bundle: GameData,
) -> Result<(), DeploymentError> {
    let (name, dir) = ask_for_props(sink);
    let mod_path = mod_path.join(dir);

    info!("Mod is being deployed to {:?}", mod_path);
    // This is possibly subject for TOCTOU attack, but in this case the user seems to have a problem somewhere else
    if mod_path.exists() {
        match ask_for_overwrite(sink, &mod_path) {
            OverwriteChoice::Overwrite => {
                info!("Overwriting existing mod bundle");
                std::fs::remove_dir_all(&mod_path).map_err(DeploymentError::from_io(&mod_path))?
            }
            OverwriteChoice::Cancel => return Err(DeploymentError::AlreadyExists),
            OverwriteChoice::Retry => {
                if mod_path.exists() {
                    return Err(DeploymentError::AlreadyExists);
                }
            }
        }
    }

    std::fs::create_dir(&mod_path).map_err(DeploymentError::from_io(&mod_path))?;

    let project_xml_path = mod_path.join("project.xml");
    let project_xml = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<project>
    <Title>{}</Title>
    <ModDataPath>{}</ModDataPath>
	<UploadMode>dont_submit</UploadMode>
</project>"#,
        name,
        mod_path
            .to_str()
            .expect("Path to the mods directory is not valid UTF-8 - this won't work"),
    );
    std::fs::write(&project_xml_path, project_xml)
        .map_err(DeploymentError::from_io(&project_xml_path))?;
    info!("Written project.xml");

    for (path, item) in bundle {
        info!("Writing mod file to relative path {:?}", path);
        super::set_file_updated(sink, "Deploying", path.to_string_lossy());
        let target = mod_path.join(path);
        let dir = target.parent().unwrap();
        std::fs::create_dir_all(dir).map_err(DeploymentError::from_io(&dir))?;
        match item {
            GameDataItem::Binary(source) => {
                info!("Copying binary file from {:?}", source);
                let mut source =
                    std::fs::File::open(&source).map_err(DeploymentError::from_io(&source))?;
                let mut target =
                    std::fs::File::create(&target).map_err(DeploymentError::from_io(&target))?;
                std::io::copy(&mut source, &mut target).map(|_| {})
            }
            GameDataItem::Structured(item) => item.deploy(&target),
        }
        .map_err(DeploymentError::from_io(&target))?;
    }
    Ok(())
}

fn ask_for_props(sink: &mut cursive::CbSink) -> (String, String) {
    let (sender, receiver) = bounded(0);

    crate::run_update(sink, move |cursive| {
        crate::push_screen(
            cursive,
            Dialog::around(
                LinearLayout::vertical()
                    .child(
                        Panel::new(
                            EditView::new()
                                .on_edit(|cursive, name, _| {
                                    cursive.call_on_name("Mod directory", |edit: &mut EditView| {
                                        edit.set_content(name.to_lowercase().replace(' ', "_"));
                                    });
                                })
                                .content("Generated bundle")
                                .with_name("Mod name")
                                .full_width(),
                        )
                        .title("Mod name"),
                    )
                    .child(
                        Panel::new(
                            EditView::new()
                                .content("generated_bundle")
                                .with_name("Mod directory")
                                .full_width(),
                        )
                        .title("Mod directory"),
                    ),
            )
            .title("Deployment parameters")
            .button("Clear", |cursive| {
                let _ =
                    cursive.call_on_name("Mod name", |view: &mut EditView| view.set_content(""));
                let _ = cursive
                    .call_on_name("Mod directory", |view: &mut EditView| view.set_content(""));
            })
            .button("Deploy!", move |cursive| {
                let name = cursive
                    .call_on_name("Mod name", |view: &mut EditView| view.get_content())
                    .unwrap();
                let dir = cursive
                    .call_on_name("Mod directory", |view: &mut EditView| view.get_content())
                    .unwrap();
                cursive.pop_layer();
                sender.send((name.to_string(), dir.to_string())).unwrap();
            }),
        )
    });

    receiver
        .recv()
        .expect("Sender was dropped without sending anything")
}

fn send_choice(sender: &Sender<OverwriteChoice>, choice: OverwriteChoice) -> impl Fn(&mut Cursive) {
    let sender = sender.clone();
    move |cursive| {
        cursive.pop_layer();
        let _ = sender.send(choice);
    }
}

fn ask_for_overwrite(sink: &mut cursive::CbSink, path: &Path) -> OverwriteChoice {
    use OverwriteChoice::*;
    let (sender, receiver) = bounded(0);
    let path = path.to_owned();
    crate::run_update(sink, move |cursive| {
        crate::push_screen(
            cursive,
            Dialog::around(TextView::new(format!(
                "Target directory {} already exists!
Choose your action:
- overwrite existing folder;
- rename/move it manually and retry deploying (it will fail if folder still exists);
- cancel mod bundling process entirely.",
                path.to_string_lossy()
            )))
            .button("Overwrite", send_choice(&sender, Overwrite))
            .button("Retry", send_choice(&sender, Retry))
            .button("Cancel", send_choice(&sender, Cancel))
            .h_align(cursive::align::HAlign::Center),
        )
    });

    receiver
        .recv()
        .expect("Sender was dropped without sending anything")
}
