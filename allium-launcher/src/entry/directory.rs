use std::{
    ffi::OsStr,
    fs::File,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Result};
use common::{constants::ALLIUM_GAMES_DIR, database::Database};
use serde::{Deserialize, Serialize};

use crate::{
    consoles::ConsoleMapper,
    entry::{game::Game, gamelist::GameList, short_name, Entry},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Directory {
    pub name: String,
    pub full_name: String,
    pub path: PathBuf,
}

impl Ord for Directory {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.full_name.cmp(&other.full_name)
    }
}

impl PartialOrd for Directory {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Default for Directory {
    fn default() -> Self {
        Directory {
            name: "Games".to_string(),
            full_name: "Games".to_string(),
            path: ALLIUM_GAMES_DIR.to_owned(),
        }
    }
}

impl Directory {
    pub fn new(path: PathBuf) -> Directory {
        let full_name = path
            .file_stem()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or("")
            .to_string();
        let name = short_name(&full_name);
        Directory {
            name,
            full_name,
            path,
        }
    }

    pub fn with_name(path: PathBuf, name: String) -> Directory {
        let full_name = path
            .file_stem()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or("")
            .to_string();
        Directory {
            name,
            full_name,
            path,
        }
    }

    fn parse_game_list(&self, game_list: &Path) -> Result<Vec<Entry>> {
        let file = File::open(game_list)?;
        let gamelist: GameList = serde_xml_rs::from_reader(file)?;

        let games = gamelist.games.into_iter().filter_map(|game| {
            let path = self.path.join(&game.path);
            if !path.exists() {
                return None;
            }

            let extension = game
                .path
                .extension()
                .and_then(OsStr::to_str)
                .unwrap_or_default()
                .to_owned();

            let full_name = game.name.clone();

            let image = game.image.map(|p| self.path.join(p)).filter(|p| p.exists());

            Some(Entry::Game(Game {
                path,
                name: game.name,
                full_name,
                image: Some(image),
                extension,
            }))
        });

        let folders = gamelist.folders.into_iter().filter_map(|folder| {
            let path = self.path.join(&folder.path);
            if !path.exists() {
                return None;
            }

            let name = folder.name;

            Some(Entry::Directory(Directory::with_name(path, name)))
        });

        Ok(folders.chain(games).collect())
    }

    pub fn entries(&self, console_mapper: &ConsoleMapper) -> Result<Vec<Entry>> {
        let gamelist = self.path.join("gamelist.xml");
        if gamelist.exists() {
            return self.parse_game_list(&gamelist);
        }

        let gamelist = self.path.join("miyoogamelist.xml");
        if gamelist.exists() {
            return self.parse_game_list(&gamelist);
        }

        let entries: Vec<_> = std::fs::read_dir(&self.path)
            .map_err(|e| anyhow!("Failed to open directory: {:?}, {}", &self.path, e))?
            .filter_map(std::result::Result::ok)
            .filter_map(|entry| match Entry::new(entry.path(), console_mapper) {
                Ok(Some(entry)) => Some(entry),
                _ => None,
            })
            .collect();

        Ok(entries)
    }

    pub fn populate_db(&self, database: &Database, console_mapper: &ConsoleMapper) -> Result<()> {
        let entries = self.entries(console_mapper)?;

        for entry in &entries {
            match entry {
                Entry::Directory(dir) => dir.populate_db(database, console_mapper)?,
                Entry::Game(_) | Entry::App(_) => {}
            }
        }

        let games: Vec<_> = entries
            .into_iter()
            .filter_map(|entry| match entry {
                Entry::Game(game) => Some(common::database::NewGame {
                    name: game.name,
                    path: game.path,
                    image: game.image.flatten(),
                }),
                _ => None,
            })
            .collect();
        database.update_games(&games)?;
        Ok(())
    }
}

impl From<&Path> for Directory {
    fn from(path: &Path) -> Self {
        Directory::new(path.into())
    }
}
