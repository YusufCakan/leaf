use super::{Anot, CustomType, FunctionBuilder, Identifier, ParseFault, Type};
use crate::env::Environment;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::fmt;
use std::path::PathBuf;
use termion::color::{Fg, Green, Reset};

// Files can be loaded either from relative path or leafpath
#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub enum FileSource {
    Project(Vec<String>),
    Leafpath(Vec<String>),
    Prelude,
}

// An entire leaf module, which represents one singular .lf file.
pub struct ParseModule {
    //                     identifer       parameters
    pub function_ids: HashMap<String, HashMap<Vec<Type>, usize>>,
    pub functions: Vec<FunctionBuilder>,

    pub type_ids: HashMap<String, usize>,
    pub types: Vec<CustomType>,

    pub imports: HashMap<String, usize>,

    pub module_path: FileSource,
}

impl ParseModule {
    pub fn new(module_path: FileSource) -> Self {
        Self {
            function_ids: HashMap::new(),
            functions: Vec::new(),
            type_ids: HashMap::new(),
            types: Vec::new(),
            imports: HashMap::new(),
            module_path,
        }
    }
    pub fn get_import(&self, name: &str) -> Result<usize, ParseFault> {
        self.imports
            .get(name)
            .cloned()
            .ok_or_else(|| ParseFault::ModuleNotImported(name.to_owned()))
    }
}

impl FileSource {
    pub fn join(self, next: String) -> Self {
        match self {
            FileSource::Project(mut levels) => {
                levels.push(next);
                FileSource::Project(levels)
            }
            FileSource::Leafpath(mut levels) => {
                levels.push(next);
                FileSource::Leafpath(levels)
            }
            FileSource::Prelude => panic!("Use statements in prelude unsupported"),
        }
    }
    pub fn pop(&mut self) -> Option<String> {
        match self {
            FileSource::Project(levels) => levels.pop(),
            FileSource::Leafpath(levels) => levels.pop(),
            FileSource::Prelude => panic!("Use statements in prelude unsupported"),
        }
    }

    pub fn to_pathbuf<'a>(&'a self, env: &Environment) -> PathBuf {
        match self {
            FileSource::Project(levels) => {
                let mut path = env.entrypoint.parent().unwrap().join(levels.join("/"));
                path.set_extension("lf");
                path
            }
            FileSource::Leafpath(levels) => {
                let mut path = env.leafpath.join("modules").join(levels.join("/"));
                path.set_extension("lf");
                path
            }
            FileSource::Prelude => panic!("Use statements in prelude unsupported"),
        }
    }

    pub fn is_entrypoint(&self) -> bool {
        if let FileSource::Project(path) = self {
            path.len() == 1
        } else {
            false
        }
    }

    // Create a new FileSource from the scope of self
    // We search for filepath both from $LEAFPATH and relatively from entrypoint
    pub fn fork_from(&self, ident: Anot<Identifier, Type>, env: &Environment) -> Self {
        if self.is_entrypoint() {
            FileSource::try_from((&ident, env)).unwrap()
        } else {
            let mut new_module_path = self.clone();
            new_module_path.pop();
            for level in ident.inner.path.into_iter() {
                new_module_path = new_module_path.join(level);
            }
            new_module_path
        }
    }
}

impl fmt::Display for FileSource {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            FileSource::Project(levels) => write!(f, "project:{}", levels.join(":")),
            FileSource::Leafpath(levels) => write!(f, "leaf:{}", levels.join(":")),
            FileSource::Prelude => write!(f, "prelude"),
        }
    }
}

impl TryFrom<(&Anot<Identifier, Type>, &Environment)> for FileSource {
    type Error = ();

    fn try_from(
        (ident, env): (&Anot<Identifier, Type>, &Environment),
    ) -> Result<FileSource, Self::Error> {
        let mut from_project_path = env.entrypoint.parent().unwrap().to_owned();

        let mut file_postfix = ident.inner.path.join("/");
        file_postfix.push('/');
        file_postfix.push_str(&ident.inner.name);
        file_postfix.push_str(".lf");

        from_project_path.push(&file_postfix);

        if from_project_path.exists() {
            let mut buf = Vec::with_capacity(ident.inner.path.len() + 1);
            for p in ident.inner.path.iter().cloned() {
                buf.push(p);
            }
            buf.push(ident.inner.name.clone());
            return Ok(FileSource::Project(buf));
        }

        let mut from_leaf_path = env.leafpath.clone();
        from_leaf_path.push("modules");
        from_leaf_path.push(file_postfix);

        if from_leaf_path.exists() {
            let mut buf = Vec::with_capacity(ident.inner.path.len() + 1);
            for p in ident.inner.path.iter().cloned() {
                buf.push(p);
            }
            buf.push(ident.inner.name.clone());
            return Ok(FileSource::Leafpath(buf));
        }

        panic!("ET: File {:?} not found", ident.inner.name);
    }
}

impl fmt::Debug for ParseModule {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "IMPORTS:\n{}\nTYPES:\n {}\nFUNCTIONS:\n{}",
            self.imports
                .iter()
                .map(|(name, fid)| format!(" {} -> {}", name, fid))
                .collect::<Vec<String>>()
                .join("\n"),
            self.type_ids
                .iter()
                .map(|(tname, tid)| format!("  #{} {}\n{}", tid, tname, "TODO",))
                .collect::<Vec<String>>()
                .join("\n"),
            self.functions
                .iter()
                .map(|funcb| format!("  {:?}", funcb))
                .collect::<Vec<String>>()
                .join("\n")
        )
    }
}

impl fmt::Display for ParseModule {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{g}IMPORTS{r}:\n{}\n{g}TYPES{r}:\n{}\n{g}FUNCTIONS{r}:\n{}",
            self.imports
                .iter()
                .map(|(name, fid)| format!(" {} -> {}", name, fid))
                .collect::<Vec<String>>()
                .join("\n"),
            self.type_ids
                .iter()
                .map(|(tname, tid)| format!("#{} {}{}", tid, tname, &self.types[*tid]))
                .collect::<Vec<String>>()
                .join("\n"),
            self.functions
                .iter()
                .map(|funcb| format!("{:?}  {}", funcb, &funcb.body))
                .collect::<Vec<String>>()
                .join("\n"),
            r = Fg(Reset),
            g = Fg(Green),
        )
    }
}
