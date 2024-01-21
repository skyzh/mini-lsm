mod wrapper;
use wrapper::mini_lsm_wrapper;

use anyhow::Result;
use bytes::Bytes;
use clap::{Parser, ValueEnum};
use mini_lsm_wrapper::compact::{
    CompactionOptions, LeveledCompactionOptions, SimpleLeveledCompactionOptions,
    TieredCompactionOptions,
};
use mini_lsm_wrapper::iterators::StorageIterator;
use mini_lsm_wrapper::lsm_storage::{LsmStorageOptions, MiniLsm};
use std::path::PathBuf;

#[derive(Debug, Clone, ValueEnum)]
enum CompactionStrategy {
    Simple,
    Leveled,
    Tiered,
    None,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, default_value = "lsm.db")]
    path: PathBuf,
    #[arg(long, default_value = "leveled")]
    compaction: CompactionStrategy,
    #[arg(long, default_value = "true")]
    enable_wal: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let lsm = MiniLsm::open(
        args.path,
        LsmStorageOptions {
            block_size: 4096,
            target_sst_size: 2 << 20, // 2MB
            num_memtable_limit: 3,
            compaction_options: match args.compaction {
                CompactionStrategy::None => CompactionOptions::NoCompaction,
                CompactionStrategy::Simple => {
                    CompactionOptions::Simple(SimpleLeveledCompactionOptions {
                        size_ratio_percent: 200,
                        level0_file_num_compaction_trigger: 2,
                        max_levels: 4,
                    })
                }
                CompactionStrategy::Tiered => CompactionOptions::Tiered(TieredCompactionOptions {
                    num_tiers: 3,
                    max_size_amplification_percent: 200,
                    size_ratio: 1,
                    min_merge_width: 2,
                }),
                CompactionStrategy::Leveled => {
                    CompactionOptions::Leveled(LeveledCompactionOptions {
                        level0_file_num_compaction_trigger: 2,
                        max_levels: 4,
                        base_level_size_mb: 128,
                        level_size_multiplier: 2,
                    })
                }
            },
            enable_wal: args.enable_wal,
        },
    )?;
    let mut epoch = 0;
    loop {
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        let line = line.trim().to_string();
        if line.starts_with("fill ") {
            let Some((_, options)) = line.split_once(' ') else {
                println!("invalid command");
                continue;
            };
            let Some((begin, end)) = options.split_once(' ') else {
                println!("invalid command");
                continue;
            };
            let begin = begin.parse::<u64>()?;
            let end = end.parse::<u64>()?;

            for i in begin..=end {
                lsm.put(
                    format!("{}", i).as_bytes(),
                    format!("value{}@{}", i, epoch).as_bytes(),
                )?;
            }

            println!("{} values filled with epoch {}", end - begin + 1, epoch);
        } else if line.starts_with("get ") {
            let Some((_, key)) = line.split_once(' ') else {
                println!("invalid command");
                continue;
            };
            if let Some(value) = lsm.get(key.as_bytes())? {
                println!("{}={:?}", key, value);
            } else {
                println!("{} not exist", key);
            }
        } else if line.starts_with("scan ") {
            let Some((_, rest)) = line.split_once(' ') else {
                println!("invalid command");
                continue;
            };
            let Some((begin_key, end_key)) = rest.split_once(' ') else {
                println!("invalid command");
                continue;
            };
            let mut iter = lsm.scan(
                std::ops::Bound::Included(begin_key.as_bytes()),
                std::ops::Bound::Included(end_key.as_bytes()),
            )?;
            while iter.is_valid() {
                println!(
                    "{:?}={:?}",
                    Bytes::copy_from_slice(iter.key()),
                    Bytes::copy_from_slice(iter.value()),
                );
                iter.next()?;
            }
        } else if line == "dump" {
            lsm.dump_structure();
        } else if line == "flush" {
            lsm.force_flush()?;
        } else if line == "quit" {
            lsm.close()?;
            break;
        } else {
            println!("invalid command: {}", line);
        }
        epoch += 1;
    }
    Ok(())
}
