// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::class_name::*;

#[derive(Debug, Clone)]
pub struct ClassWrapperSpec {
    class_name: ClassName,
    items: Vec<ItemWrapperSpec>,
}

impl ClassWrapperSpec {
    pub fn new(full_class_name: &str, items: Vec<ItemWrapperSpec>) -> Self {
        ClassWrapperSpec {
            class_name: ClassName::new(full_class_name),
            items,
        }
    }

    pub fn get_name(&self) -> &ClassName {
        &self.class_name
    }

    pub fn get_items(&self) -> &Vec<ItemWrapperSpec> {
        &self.items
    }
}

#[derive(Debug, Clone)]
pub enum ItemWrapperSpec {
    ScalaObjectGetter(),
    Constructor {
        signature: Option<String>,
        suffix: Option<String>,
    },
    Method {
        name: String,
        signature: Option<String>,
        suffix: Option<String>,
    },
}

#[macro_export]
macro_rules! java_class {
    ($full_class_name:expr) => {
        ClassWrapperSpec::new($full_class_name, vec![])
    };
    ($full_class_name:expr, $items:expr) => {
        ClassWrapperSpec::new($full_class_name, $items)
    };
}

#[macro_export]
macro_rules! object_getter {
    () => {
        ItemWrapperSpec::ScalaObjectGetter()
    };
}

#[macro_export]
macro_rules! constructor {
    () => {
        ItemWrapperSpec::Constructor {
            signature: None,
            suffix: None,
        }
    };
    ($signature:expr) => {
        ItemWrapperSpec::Constructor {
            signature: Some($signature.into()),
            suffix: None,
        }
    };
    ($signature:expr, $suffix:expr) => {
        ItemWrapperSpec::Constructor {
            signature: $signature.into(),
            suffix: Some($suffix.into()),
        }
    };
}

#[macro_export]
macro_rules! method {
    ($name:expr) => {
        ItemWrapperSpec::Method {
            name: $name.into(),
            signature: None,
            suffix: None,
        }
    };
    ($name:expr, $signature:expr) => {
        ItemWrapperSpec::Method {
            name: $name.into(),
            signature: Some($signature.into()),
            suffix: None,
        }
    };
    ($name:expr, $signature:expr, $suffix:expr) => {
        ItemWrapperSpec::Method {
            name: $name.into(),
            signature: Some($signature.into()),
            suffix: Some($suffix.into()),
        }
    };
}
