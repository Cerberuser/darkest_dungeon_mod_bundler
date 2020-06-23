use super::{
    diff::{DataNodeContent, DataTree},
    error::DeploymentError,
};
use crossbeam_channel::{bounded, Sender};
use cursive::{
    views::{Dialog, TextView},
    Cursive,
};
use indoc::indoc;
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
    bundle: DataTree,
) -> Result<(), DeploymentError> {
    info!("Mod is being deployed to {:?}", mod_path);
    // This is possibly subject for TOCTOU attack, but in this case the user seems to have a problem somewhere else
    if mod_path.exists() {
        match ask_for_overwrite(sink, mod_path) {
            OverwriteChoice::Overwrite => {
                info!("Overwriting existing mod bundle");
                std::fs::remove_dir_all(mod_path).map_err(DeploymentError::from_io(&mod_path))?
            }
            OverwriteChoice::Cancel => return Err(DeploymentError::AlreadyExists),
            OverwriteChoice::Retry => {
                if mod_path.exists() {
                    return Err(DeploymentError::AlreadyExists);
                }
            }
        }
    }

    std::fs::create_dir(mod_path).map_err(DeploymentError::from_io(mod_path))?;

    let project_xml_path = mod_path.join("project.xml");
    std::fs::write(
        &project_xml_path,
        indoc!(
            r#"
            <?xml version="1.0" encoding="utf-8"?>
            <project>
                <Title>Generated mods bundle</Title>
            </project>
            "#
        ),
    )
    .map_err(DeploymentError::from_io(&project_xml_path))?;
    info!("Written project.xml");

    for (path, item) in bundle {
        info!("Writing mod file to relative path {:?}", path);
        super::set_file_updated(sink, "Deploying", path.to_string_lossy());
        let (source, content) = item.into_parts();
        let target = mod_path.join(path);
        let dir = target.parent().unwrap();
        std::fs::create_dir_all(dir).map_err(DeploymentError::from_io(&dir))?;
        match content {
            DataNodeContent::Binary => {
                info!("Copying binary file from {:?}", source);
                let mut source =
                    std::fs::File::open(&source).map_err(DeploymentError::from_io(&source))?;
                let mut target =
                    std::fs::File::create(&target).map_err(DeploymentError::from_io(&target))?;
                std::io::copy(&mut source, &mut target).map(|_| {})
            }
            DataNodeContent::Text(text) => {
                info!(
                    "Writing text file, first 100 chars = \"{}\"",
                    text.chars().take(100).collect::<String>()
                );
                std::fs::write(&target, text)
            }
        }
        .map_err(DeploymentError::from_io(&target))?;
    }
    Ok(())
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
