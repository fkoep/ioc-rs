use std::fmt::{self, Display};

#[derive(Debug, Clone)]
pub enum PathComponent {
    Persistent(String),
    Transient(Option<String>),
    // Repository(String),
    // Instancer(String),
}

/// TODO Remove this?
///
/// TODO impl Debug, Display
#[derive(Debug, Clone)]
pub struct Path {
    coms: Vec<PathComponent>,
}

impl Path {
    pub fn new(root: String) -> Self { Self { coms: vec![PathComponent::Persistent(root)] } }

    pub fn push(&mut self, com: PathComponent) {
        use self::PathComponent as Com;

        match (&com, self.coms.last().unwrap()) {
            (&Com::Persistent(_), &Com::Persistent(_))
            | (&Com::Persistent(_), &Com::Transient(_)) 
            | (&Com::Transient(_), &Com::Persistent(_))
            // | (&Com::Repository(_), &Com::Transient(_))
            // | (&Com::Instancer(_), &Com::Repository(_))
            => { 
                /* all ok */
            },
            (com, last) => {
                panic!("{:?} may not follow {:?}!", com, last);
            },
        }

        self.coms.push(com)
    }

    // pub fn is_node(&self) -> bool {
    //     match self.coms.last().unwrap() {
    //         &PathComponent::Persistent(_) | &PathComponent::Transient(_) => true,
    //         _ => false
    //     }
    // }
    pub fn is_root(&self) -> bool { self.coms.len() == 1 }
    pub fn is_persistent(&self) -> bool {
        if let &PathComponent::Persistent(_) = self.coms.last().unwrap() {
            true
        } else {
            false
        }
    }
    pub fn is_transient(&self) -> bool {
        if let &PathComponent::Transient(_) = self.coms.last().unwrap() {
            true
        } else {
            false
        }
    }

    // pub fn node(&self) -> &[PathComponent] {
    //     let end = self.coms.iter().position(|com| {
    //         if let &PathComponent::Repository(_) = com { true } else { false }
    //     }).unwrap_or(self.coms.len());
    //     &self.coms[0..end]
    // }
    // pub fn repository(&self) -> Option<&String> {
    //     self.coms.iter().filter_map(|com| {
    // if let &PathComponent::Repository(ref r) = com { Some(r) } else {
    // None }
    //     }).next()
    // }
    // pub fn instancer(&self) -> Option<&String> {
    //     match self.coms.last() {
    //         Some(&PathComponent::Repository(ref r)) => Some(r),
    //         _ => None
    //     }
    // }
}
