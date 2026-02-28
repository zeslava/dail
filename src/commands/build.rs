use clap::Args;
use crate::build::executor::BuildExecutor;
use crate::build::dailfile::Dailfile;
use crate::image::ImageRef;
use crate::jail::config::GlobalConfig;
use crate::jail::lifecycle::JailLifecycle;

#[derive(Args)]
#[command(after_long_help = "\
Examples:
  dail build --name myapp                     Use ./Dailfile in current dir
  dail build Dailfile --name myapp
  dail build ./jails/web.dailfile --name web
  dail build Dailfile --tag postgres:18")]
pub struct BuildArgs {
    /// Path to Dailfile (default: ./Dailfile)
    pub dailfile: Option<String>,
    /// Jail name (required without --tag, auto-generated with --tag)
    #[arg(long)]
    pub name: Option<String>,
    /// Save result as image (name:tag) and remove temp jail
    #[arg(long)]
    pub tag: Option<String>,
}

pub fn run(args: BuildArgs) -> anyhow::Result<()> {
    let global = GlobalConfig::load()?;
    let mut lifecycle = JailLifecycle::new(global.clone())?;

    let name = match (&args.name, &args.tag) {
        (Some(n), _) => n.clone(),
        (None, Some(_)) => format!("dail-build-{}", &uuid::Uuid::new_v4().to_string()[..8]),
        (None, None) => anyhow::bail!("--name is required when not using --tag"),
    };

    let dailfile_path = args.dailfile.unwrap_or_else(|| "Dailfile".to_string());
    let content = std::fs::read_to_string(&dailfile_path)
        .map_err(|_| anyhow::anyhow!("Dailfile not found: {dailfile_path}"))?;
    let dailfile = Dailfile::parse(&content)?;

    let context_dir = std::path::Path::new(&dailfile_path)
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .canonicalize()
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    BuildExecutor::build(&mut lifecycle, &dailfile, &name, None, &context_dir)?;

    if let Some(ref tag_ref) = args.tag {
        let image_ref = ImageRef::parse(tag_ref)?;
        let img_name = &image_ref.name;
        let img_tag = &image_ref.tag;

        let state = lifecycle.get(&name)
            .ok_or_else(|| anyhow::anyhow!("built jail '{}' not found in store", name))?
            .clone();

        let image_dir = global.images_dir().join(img_name).join(img_tag);
        std::fs::create_dir_all(&image_dir)?;
        let output_path = image_dir.join("image.tar.zst");

        // Save with the desired image name in manifest
        let manifest = crate::image::ImageManifest {
            name: img_name.to_string(),
            tag: img_tag.to_string(),
            base: state.config.base.clone(),
            params: state.config.params.clone(),
            limits: state.config.limits.clone(),
            persist: state.config.persist,
            cmd: state.config.cmd.clone(),
            created_at: chrono::Utc::now(),
        };
        let manifest_json = serde_json::to_string_pretty(&manifest)
            .map_err(|e| anyhow::anyhow!("failed to serialize manifest: {e}"))?;
        std::fs::write(image_dir.join("manifest.json"), &manifest_json)?;

        // Create archive from jail root
        crate::image::tar_create_zstd(
            &[(state.root_path.as_path(), &["."])],
            &output_path,
        )?;

        lifecycle.remove(&name, true)?;
        println!("Image saved as {}:{}", img_name, img_tag);
    } else {
        println!("Jail '{}' built successfully", name);
    }

    Ok(())
}
