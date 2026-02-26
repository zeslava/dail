use clap::Subcommand;
use clap_complete::engine::ArgValueCompleter;

use crate::completions;
use crate::image::ImageStore;
use crate::jail::config::GlobalConfig;
use crate::store::DailStore;

#[derive(Subcommand)]
pub enum ImageAction {
    /// Export a jail as a portable image archive
    Save {
        /// Jail name
        #[arg(add = ArgValueCompleter::new(completions::complete_jail_names))]
        jail: String,
        /// Image tag
        #[arg(short, long, default_value = "latest")]
        tag: String,
        /// Output file path
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Import an image from a tar.zst archive
    Load {
        /// Path to archive file
        file: String,
    },
    /// List local images
    Ls,
    /// Remove a local image (name:tag)
    Rm {
        /// Image reference (name:tag)
        #[arg(add = ArgValueCompleter::new(completions::complete_image_refs))]
        image: String,
    },
    /// Show image manifest details
    Inspect {
        /// Image reference (name:tag)
        #[arg(add = ArgValueCompleter::new(completions::complete_image_refs))]
        image: String,
    },
}

pub fn run(action: ImageAction) -> anyhow::Result<()> {
    let config = GlobalConfig::load()?;

    match action {
        ImageAction::Save { jail, tag, output } => {
            let store = DailStore::new(&config)?;
            let state = store
                .get(&jail)
                .ok_or_else(|| anyhow::anyhow!("jail not found: {}", jail))?;

            let image_store = ImageStore::new(&config);
            let out = output.as_deref().map(std::path::Path::new);
            let path = image_store.save(state, &tag, out)?;
            println!("Image saved: {}", path.display());
        }
        ImageAction::Load { file } => {
            let image_store = ImageStore::new(&config);
            let manifest = image_store.load(std::path::Path::new(&file))?;
            println!("Loaded image: {}:{}", manifest.name, manifest.tag);
        }
        ImageAction::Ls => {
            let image_store = ImageStore::new(&config);
            let images = image_store.list()?;
            if images.is_empty() {
                println!("No images found");
                return Ok(());
            }
            println!("{:<20} {:<10} {:<15} {}", "NAME", "TAG", "BASE", "CREATED");
            for img in &images {
                println!(
                    "{:<20} {:<10} {:<15} {}",
                    img.name,
                    img.tag,
                    img.base.as_deref().unwrap_or("-"),
                    img.created_at.format("%Y-%m-%d %H:%M"),
                );
            }
        }
        ImageAction::Inspect { image } => {
            let (name, tag) = image
                .split_once(':')
                .ok_or_else(|| anyhow::anyhow!("invalid format, expected name:tag"))?;
            let image_store = ImageStore::new(&config);
            let manifest = image_store.load_manifest(name, tag)?;
            println!("{}", serde_json::to_string_pretty(&manifest)?);
        }
        ImageAction::Rm { image } => {
            let (name, tag) = image
                .split_once(':')
                .ok_or_else(|| anyhow::anyhow!("invalid format, expected name:tag"))?;

            let store = DailStore::new(&config)?;
            let image_ref = format!("{name}:{tag}");
            let dependents: Vec<String> = store
                .list()
                .iter()
                .filter(|s| s.config.image_ref.as_deref() == Some(image_ref.as_str()))
                .map(|s| s.name().to_string())
                .collect();
            if !dependents.is_empty() {
                anyhow::bail!(
                    "image {} is used by thin jails: {}. Remove them first or use --type thick.",
                    image, dependents.join(", ")
                );
            }

            let image_store = ImageStore::new(&config);
            image_store.remove(name, tag)?;
            println!("Removed image: {}", image);
        }
    }

    Ok(())
}
