#[macro_use] extern crate log;
use std::path::{Path, PathBuf};
use std::fs::File;
use std::io::{Read, Write};
use std::str::FromStr;
use clap::Parser;
use chrono::Local;
use fern::Dispatch;
use log::LevelFilter;
use snafu::{whatever, ResultExt};
use tsk::{ReadSeek, TskImgReadSeek, TskImg};
use awssnapio::{DiskCache, AwsSnapshot};
use tokio::runtime::{Handle, Runtime};

static VERSION: &str = env!("CARGO_PKG_VERSION");

pub type Result<T, E = snafu::Whatever> = std::result::Result<T, E>;


/// A tool that can extract EVTX files from an AWS snapshot. Use the AWS CLI
/// to setup your environment first.
#[derive(Parser, Debug)]
#[command(
    author = "Matthew Seyer",
    version=VERSION,
)]
struct App {
    /// The source to extract EVTX files from. This can be a snapshot or a dd.
    #[arg(short, long, required=true)]
    source: PathBuf,
    /// The output directory to write the EVTX files to.
    #[arg(short, long, required=true)]
    output: PathBuf,
    /// The location to store the disk cache.
    #[arg(short, long, required=true)]
    disk_cache: PathBuf,
    /// The logging level to use.
    #[arg(long, default_value="Info", value_parser=["Off", "Error", "Warn", "Info", "Debug", "Trace"])]
    logging: String,
}
impl App {
    fn set_logging(&self) -> Result<(), snafu::Whatever> {
        let level = self.logging.as_str();

        let message_level = LevelFilter::from_str(level)
            .with_whatever_context(|e|format!("Could not set logging level: {e:?}"))?;

        // Create logging with debug level that prints to stderr
        // See https://docs.rs/fern/0.6.0/fern/#example-setup
        let result = Dispatch::new()
            .format(|out, message, record| {
                out.finish(format_args!(
                    "{}[{}][{}] {}",
                    Local::now().format("[%Y-%m-%d %H:%M:%S]"),
                    record.target(),
                    record.level(),
                    message
                ))
            })
            .level(message_level)
            .level_for("aws_smithy_http_tower", log::LevelFilter::Off)
            .level_for("aws_endpoint", log::LevelFilter::Off)
            .level_for("aws_config", log::LevelFilter::Off)
            .level_for("hyper", log::LevelFilter::Off)
            .chain(std::io::stderr())
            .apply();
        
        // Ensure that logger was dispatched
        match result {
            Ok(_) => trace!("Logging has been initialized!"),
            Err(error) => {
                whatever!("Error initializing fern logging: {}", error);
            }
        }

        Ok(())
    }
}


async fn box_from_snapshot<'r>(
    source: &str,
    disk_cache: DiskCache,
    handle: Box<Handle>
) -> Result<(Box<dyn ReadSeek + 'r>, i64)> {
    let timeout_config = aws_config::timeout::TimeoutConfig::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .read_timeout(std::time::Duration::from_secs(5))
        .operation_attempt_timeout(std::time::Duration::from_secs(5))
        .operation_timeout(std::time::Duration::from_secs(5))
        .build();

    let retry_config = aws_config::retry::RetryConfig::disabled();

    let sdk_config = aws_config::from_env()
        .timeout_config(timeout_config)
        .retry_config(retry_config)
        .load()
        .await;

    let snapshot = AwsSnapshot::from_snashot_id(
            sdk_config,
            source
        )
        .await
        .with_whatever_context(|e|format!("{:?}", e))?;

    let vol_size = snapshot.volume_size_as_bytes()
        .try_into()
        .whatever_context(format!("Could not convert u64 to i64"))?;

    let snapshot_handle = snapshot.into_handle(handle, Some(disk_cache))
        .await
        .expect("Error getting handle from AwsSnapshot");

    Ok((Box::new(snapshot_handle), vol_size))
}


fn box_from_file(source: impl AsRef<Path>) -> Result<(Box<dyn ReadSeek>, i64)> {
    let source = source.as_ref();

    let handle = std::fs::File::open(source)
        .with_whatever_context(|e|format!(
            "Could not open file! {}; {:?}", &source.to_string_lossy(), e)
        )?;

    let size = source
        .metadata()
        .with_whatever_context(|e|format!("Could not get meta for path: {:?}", e))?
        .len()
        .try_into()
        .with_whatever_context(|e|format!("Could not convert u64 into i64. {:?}", e))?;
    
    Ok( (Box::new(handle), size) )
}


fn main() {
    let app: App = App::parse();
    app.set_logging()
        .expect("Error setting logging!");

    let output_folder = app.output;
    let source = app.source
        .to_string_lossy()
        .to_string();
    let tsk_source = source.clone();
    let cache = app.disk_cache;

    let disk_cache = DiskCache::from_path(cache)
        .expect("Error creating DiskCache.");

    let runtime = Runtime::new().unwrap();
    let handle = Box::new(runtime.handle().to_owned());

    let (boxed_read_seek, size): (Box<dyn ReadSeek>, i64) = if source.starts_with("snap") {
        handle.block_on(
            box_from_snapshot(source.as_str(), disk_cache, handle.clone())
        ).expect("Error getting boxed snapshot io")
    } else {
        box_from_file(source)
            .expect("Error opening file!")
    };
    debug!("Created boxed read/seek trait!");

    let tsk_img: TskImg = TskImgReadSeek::from_read_seek(tsk_source, boxed_read_seek, size)
        .expect("Error creating TskImgReadSeek")
        .into();
    debug!("Created TskImg.");

    let tsk_vs = tsk_img.get_vs_from_offset(0)
        .expect("Could not open TskVs at offset 0");
    info!("{:?}", tsk_vs);
    
    let part_iter = tsk_vs.get_partition_iter()
        .expect("Could not get partition iterator for TskVs");

    for vs_part in part_iter {
        if vs_part.desc().contains("NTFS") {
            let part_ofs = vs_part.get_start_offset();
            let tsk_fs = tsk_img.get_fs_from_offset(part_ofs)
                .expect("Could not open TskFs at offset {part_ofs}");

            let evtx_dir = tsk_fs.dir_open("/Windows/System32/winevt/Logs")
                .expect("Could not open evtx folder");
            
            for fn_name in evtx_dir.get_name_iter() {
                if let Some(name) = fn_name.name() {
                    if name.to_lowercase().ends_with(".evtx") {
                        let tsk_file = match tsk_fs.file_open_meta(fn_name.get_inode()) {
                            Err(e) => {
                                error!("Error opening {:?}: {:?}", fn_name, e);
                                continue;
                            },
                            Ok(f) => f
                        };

                        if !output_folder.as_path().exists() {
                            std::fs::create_dir_all(output_folder.as_path())
                                .expect("Could not create output folder!");
                        }

                        let mut tsk_file_handle = match tsk_file.get_attr() {
                            Ok(a) => a,
                            Err(e) => {
                                error!("Error getting default attr for {:?}: {:?}", fn_name, e);
                                continue;
                            }
                        };

                        let file_name = fn_name.name().unwrap();
                        let output_name = output_folder
                            .to_path_buf()
                            .join(&file_name);
                        
                        eprintln!("Extracting {file_name} to {}", output_name.to_string_lossy());
                        let mut output_handle = File::create(&output_name)
                            .expect("Error creating output file.");

                        let mut offset = 0;
                        while offset < tsk_file_handle.size() {
                            let mut buffer = vec![0; 1024];
                            let bytes_read = match tsk_file_handle.read(&mut buffer){
                                Ok(br) => br,
                                Err(e) => {
                                    panic!("Error reading from data attribute at offset {}. {:?}", offset, e);
                                }
                            };
                            output_handle.write(&buffer[0..bytes_read])
                                .expect("Error writing to output file.");
                                
                            offset += bytes_read as i64;
                        }
                    }
                }
            }
        }
    }
}
