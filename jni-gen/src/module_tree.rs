// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::collections::HashMap;

#[derive(Debug)]
pub enum ModuleTree {
    Node(HashMap<String, ModuleTree>),
    Leaf,
}

impl Default for ModuleTree {
    fn default() -> Self {
        ModuleTree::Node(HashMap::new())
    }
}

impl ModuleTree {
    pub fn insert<I>(self, path: I) -> Self
    where
        I: IntoIterator<Item = String>,
    {
        let mut path_iter = path.into_iter();
        let curr_name = path_iter.next();
        match curr_name {
            Some(name) => match self {
                ModuleTree::Leaf => {
                    let mut modules = HashMap::new();
                    modules.insert(name, ModuleTree::Leaf.insert(path_iter));
                    ModuleTree::Node(modules)
                }
                ModuleTree::Node(mut modules) => {
                    let sub_tree = match modules.remove(&name) {
                        None => ModuleTree::Leaf,
                        Some(sub_tree) => sub_tree,
                    };
                    modules.insert(name, sub_tree.insert(path_iter));
                    ModuleTree::Node(modules)
                }
            },
            None => self,
        }
    }

    #[allow(dead_code)]
    pub fn insert_all<I, J>(self, paths: I) -> Self
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = String>,
    {
        let mut modules = self;
        for path in paths {
            modules = modules.insert(path);
        }
        modules
    }

    #[allow(dead_code)]
    pub fn get_paths(&self) -> Vec<Vec<String>> {
        match *self {
            ModuleTree::Leaf => vec![],
            ModuleTree::Node(ref modules) => {
                let mut result: Vec<Vec<String>> = vec![];
                for (name, sub_modules) in modules.iter() {
                    let mut sub_results: Vec<Vec<String>> = sub_modules.get_paths();
                    if !sub_results.is_empty() {
                        for sub_result in &mut sub_results {
                            let mut tmp = vec![name.to_string()];
                            tmp.append(sub_result);
                            *sub_result = tmp;
                        }
                    } else {
                        sub_results = vec![vec![name.to_string()]];
                    }
                    result.extend(sub_results);
                }
                result
            }
        }
    }

    pub fn visit<F, R>(&self, f: F) -> Option<R>
    where
        F: Fn(HashMap<String, Option<R>>) -> R,
    {
        self._visit(&f)
    }

    fn _visit<F, R>(&self, f: &F) -> Option<R>
    where
        F: Fn(HashMap<String, Option<R>>) -> R,
    {
        match *self {
            ModuleTree::Leaf => None,
            ModuleTree::Node(ref modules) => {
                let mut computed: HashMap<String, Option<R>> = HashMap::new();
                for (name, sub_modules) in modules.iter() {
                    computed.insert(name.clone(), sub_modules._visit(f));
                }
                Some((*f)(computed))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unordered_set_eq::*;

    macro_rules! string_vec {
        ($($x:expr),*) => (vec![$($x.to_string()),*]);
    }

    #[test]
    fn no_path() {
        let modules = ModuleTree::default();
        let result = modules.get_paths();
        let expected: Vec<Vec<String>> = vec![];
        assert_eq!(expected, result);
    }

    #[test]
    fn single_path() {
        let path = string_vec!["a", "b", "c", "d"];
        let modules = ModuleTree::default().insert(path.clone());
        let result = modules.get_paths();
        let expected = vec![path];
        assert_eq!(expected, result);
    }

    #[test]
    fn multiple_paths() {
        let paths = vec![
            string_vec!["a"],
            string_vec!["a"],
            string_vec!["a"],
            string_vec!["b", "b"],
            string_vec!["b", "b"],
            string_vec!["c", "a"],
            string_vec!["c", "c", "b"],
            string_vec!["d", "c"],
            string_vec!["d", "b", "a", "a"],
            string_vec!["d", "b", "b", "a"],
            string_vec!["d", "a", "b", "c"],
            string_vec!["e", "a", "a", "a", "a", "a", "a"],
            string_vec!["e", "a", "a", "a", "a", "b", "a"],
        ];
        let modules = ModuleTree::default().insert_all(paths.clone());
        let result = modules.get_paths();
        assert_eq!(UnorderedSetEq(&paths), UnorderedSetEq(&result));
    }
}
