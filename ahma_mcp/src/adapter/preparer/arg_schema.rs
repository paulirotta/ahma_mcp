use std::collections::{HashMap, HashSet};

/// Fast lookup/index for option and positional argument metadata.
pub(super) struct ArgSchemaIndex<'a> {
    options_by_name: HashMap<&'a str, &'a crate::config::CommandOption>,
    positional_by_name: HashMap<&'a str, &'a crate::config::CommandOption>,
    positional_order: Vec<&'a str>,
    positional_names: HashSet<&'a str>,
    positional_args_first: bool,
}

impl<'a> ArgSchemaIndex<'a> {
    pub(super) fn new(subcommand_config: Option<&'a crate::config::SubcommandConfig>) -> Self {
        let mut options_by_name = HashMap::new();
        let mut positional_by_name = HashMap::new();
        let mut positional_order = Vec::new();
        let mut positional_names = HashSet::new();
        let mut positional_args_first = false;

        if let Some(sc) = subcommand_config {
            positional_args_first = sc.positional_args_first.unwrap_or(false);

            if let Some(options) = sc.options.as_deref() {
                for opt in options {
                    options_by_name.insert(opt.name.as_str(), opt);
                }
            }

            if let Some(positional_args) = sc.positional_args.as_deref() {
                for arg in positional_args {
                    positional_by_name.insert(arg.name.as_str(), arg);
                    positional_order.push(arg.name.as_str());
                    positional_names.insert(arg.name.as_str());
                }
            }
        }

        Self {
            options_by_name,
            positional_by_name,
            positional_order,
            positional_names,
            positional_args_first,
        }
    }

    pub(super) fn option(&self, name: &str) -> Option<&'a crate::config::CommandOption> {
        self.options_by_name.get(name).copied()
    }

    pub(super) fn positional(&self, name: &str) -> Option<&'a crate::config::CommandOption> {
        self.positional_by_name.get(name).copied()
    }

    pub(super) fn is_positional(&self, name: &str) -> bool {
        self.positional_names.contains(name)
    }

    pub(super) fn positional_args_first(&self) -> bool {
        self.positional_args_first
    }

    pub(super) fn positional_names_in_order(&self) -> impl Iterator<Item = &'a str> + '_ {
        self.positional_order.iter().copied()
    }

    pub(super) fn is_path_arg(&self, name: &str) -> bool {
        self.option(name)
            .map(|opt| opt.format.as_deref() == Some("path"))
            .unwrap_or(false)
            || self
                .positional(name)
                .map(|arg| arg.format.as_deref() == Some("path"))
                .unwrap_or(false)
    }
}
