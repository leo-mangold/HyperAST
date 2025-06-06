use axum::Json;
// use hyperast::types::LabelStore;
use hyperast_vcs_git::git::{fetch_github_repository, retrieve_commit};
use serde::{Deserialize, Serialize};

use crate::SharedState;

#[derive(Serialize, Deserialize, Clone, Debug, Hash, PartialEq, Eq)]
pub struct Param {
    pub user: String,
    pub name: String,
    /// either a commit id or a tag
    pub version: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Metadata {
    /// commit message
    message: Option<String>,
    /// parents commits
    /// if multiple parents, the first one should be where the merge happends
    parents: Vec<String>,
    /// tree corresponding to version
    tree: Option<String>,
    /// offset in minutes
    timezone: i32,
    /// seconds
    time: i64,
    /// (opt) ancestors in powers of 2; [2,4,8,16,32]
    /// important to avoid linear loading time
    pub(crate) ancestors: Vec<String>,
    pub(crate) forth_timestamp: i64,
}

// TODO prefetch a list of parent ids in power of 2 [2,4,8,16,32]
pub fn commit_metadata(_state: SharedState, path: Param) -> Result<Json<Metadata>, String> {
    let Param {
        user,
        name,
        version,
    } = path.clone();
    let repo = fetch_github_repository(&format!("{}/{}", user, name));
    log::debug!("done cloning {user}/{name}");
    let commit = retrieve_commit(&repo, &version);
    if let Err(err) = &commit {
        log::error!("{}", err.to_string());
    }
    let commit = commit.map_err(|err| err.to_string())?;
    log::debug!("done retrieving commit {version}");
    let time = commit.time();
    let timezone = time.offset_minutes();
    let time = time.seconds();
    let tree = commit.tree().ok().map(|x| x.id().to_string());
    let parents = commit.parent_ids().map(|x| x.to_string()).collect();
    let message = commit.message().map(|s| s.to_string());
    let mut forth_timestamp = i64::MAX;
    let mut ancestors = vec![commit.id().to_string()];
    let mut c = commit;
    loop {
        if ancestors.len() > 4 {
            break;
        }
        if let Ok(p) = c.parent(0) {
            if ancestors.len() == 4 {
                let time = p.time();
                let timezone = time.offset_minutes();
                let time = time.seconds();
                forth_timestamp = time + timezone as i64;
            }
            ancestors.push(p.id().to_string());
            c = p;
        } else {
            break;
        }
    }

    let ancestors = (1..2)
        .map(|i| i * i)
        .map_while(|i| ancestors.get(i).cloned())
        .collect();

    log::debug!("sending metadata of commit {version}");

    Ok(Json(Metadata {
        message,
        parents,
        tree,
        timezone,
        time,
        ancestors,
        forth_timestamp,
    }))
}

#[derive(Default)]
struct BuffOut {
    buff: String,
}

impl std::fmt::Write for BuffOut {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        Ok(self.buff.extend(s.chars()))
    }
}

impl From<BuffOut> for String {
    fn from(value: BuffOut) -> Self {
        value.buff
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Hash, PartialEq, Eq)]
pub struct ParamRemote {
    pub user: String,
    pub name: String,
    pub other_user: String,
    pub other_name: String,
    pub head: String,
}

pub fn add_remote(_state: SharedState, path: ParamRemote) -> Result<(), String> {
    let ParamRemote {
        user,
        name,
        other_user,
        other_name,
        head,
    } = path.clone();
    let repo = fetch_github_repository(&format!("{}/{}", user, name));
    let remote = format!("{}{}/{}", "https://github.com/", user, name);
    log::error!("{:?}", &remote);
    let other = format!("{}_{}", other_user, other_name);
    let r = repo.remote(&other, &remote);

    let r = match r {
        Ok(x) => {
            Ok(x)
        }
        Err(e) => {
            log::warn!("{}", e);
            if e.raw_code() == -4 {
                repo.find_remote(&other)
            } else {
                log::error!("{:?}", e);
                return Err(e.to_string())
            }
        }
    };

    match r {
        Ok(x) => {
            log::error!("{:?}", &head);
            hyperast_vcs_git::git::fetch_fork(x, &head).unwrap();
            Ok(())
        }
        Err(e) => {
            log::error!("{:?}", e);
            Err(e.to_string())
        }
    }
}
