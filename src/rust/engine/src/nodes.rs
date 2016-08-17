use std::collections::HashMap;
use std::rc::Rc;

use core::{Key, TypeId, Variants};
use selectors;
use selectors::Selector;
use tasks::Tasks;

#[derive(Debug, Eq, Hash, PartialEq)]
pub struct Runnable {
  func: Key,
  args: Vec<Key>,
}

#[derive(Debug, Eq, Hash, PartialEq)]
pub enum State {
  Waiting(Vec<Node>),
  Complete(Complete),
  Runnable(Runnable),
}

#[derive(Debug, Eq, Hash, PartialEq)]
pub enum Complete {
  Noop(String),
  Return(Key),
  Throw(String),
}

pub struct StepContext<'g,'t> {
  deps: HashMap<&'g Node, &'g Complete>,
  tasks: &'t Tasks,
}

impl<'g,'t> StepContext<'g,'t> {
  /**
   * Create Nodes for each Task that might be able to compute the given product for the
   * given subject and variants.
   *
   * (analogous to NodeBuilder.gen_nodes)
   *
   * TODO: intrinsics
   */
  fn gen_nodes(&self, subject: &Key, product: TypeId, variants: &Variants) -> Vec<Node> {
    self.tasks.get(&product).map(|tasks|
      tasks.iter()
        .map(|task| {
          Node::Task(
            Task {
              subject: subject.clone(),
              product: product,
              variants: variants.clone(),
              // TODO: cloning out of the task struct is easier than tracking references from
              // Nodes to Tasks... but should likely do it if memory usage becomes an issue.
              func: task.func().clone(),
              clause: task.input_clause().clone(),
            }
          )
        })
        .collect()
    ).unwrap_or_else(|| Vec::new())
  }

  fn get(&self, node: &Node) -> Option<&Complete> {
    self.deps.get(node).map(|c| *c)
  }

  fn key_none(&self) -> &Key {
    self.tasks.key_none()
  }

  fn type_address(&self) -> TypeId {
    self.tasks.type_address()
  }

  fn type_variants(&self) -> TypeId {
    self.tasks.type_variants()
  }

  fn has_products(&self, item: &Key) -> bool {
    self.isinstance(item, self.tasks.type_has_products())
  }

  fn field_name(&self, item: &Key) -> Key {
    self.project(item, self.tasks.key_name())
  }

  fn field_products(&self, item: &Key) -> Vec<Key> {
    self.project_multi(item, self.tasks.key_products())
  }

  fn isinstance(&self, item: &Key, superclass: TypeId) -> bool {
    panic!("TODO: not implemented!");
  }

  fn project(&self, item: &Key, field: &Key) -> Key {
    panic!("TODO: not implemented!");
  }

  fn project_multi(&self, item: &Key, field: &Key) -> Vec<Key> {
    panic!("TODO: not implemented!");
  }
}

/**
 * Defines executing a single step for the given context.
 */
trait Step {
  fn step(&self, context: StepContext) -> State;
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Select {
  subject: Key,
  variants: Variants,
  selector: selectors::Select,
}

impl Select {
  fn variant_key(&self) -> Option<&Key> {
    panic!("TODO: not implemented");
  }

  fn product(&self) -> TypeId {
    self.selector.product
  }

  fn select_literal_single<'a>(
    &self,
    context: &StepContext,
    candidate: &'a Key,
    variant_value: Option<&Key>
  ) -> Option<&'a Key> {
    if !context.isinstance(candidate, self.selector.product) {
      return None;
    }
    match variant_value {
      Some(vv) if context.field_name(candidate) != *vv =>
        // There is a variant value, and it doesn't match.
        return None,
      _ =>
        return Some(candidate),
    }
  }

  /**
   * Looks for has-a or is-a relationships between the given value and the requested product.
   *
   * Returns the resulting product value, or None if no match was made.
   */
  fn select_literal(
    &self,
    context: &StepContext,
    candidate: &Key,
    variant_value: Option<&Key>
  ) -> Option<Key> {
    // Check whether the subject is-a instance of the product.
    if let Some(candidate) = self.select_literal_single(context, candidate, variant_value) {
      return Some(candidate.clone())
    }

    // Else, check whether it has-a instance of the product.
    // TODO: returning only the first literal configuration of a given type/variant. Need to
    // define mergeability for products.
    if context.has_products(candidate) {
      for child in context.field_products(candidate) {
        if let Some(child) = self.select_literal_single(context, &child, variant_value) {
          return Some(child.clone());
        }
      }
    }
    return None;
  }
}

impl Step for Select {

  fn step(&self, context: StepContext) -> State {
    // Request default Variants for the subject, so that if there are any we can propagate
    // them to task nodes.
    if self.subject.type_id() == context.type_address() &&
       self.product() != context.type_variants() {
      panic!("TODO: not implemented.");
    }

    // If there is a variant_key, see whether it has been configured; if not, no match.
    let variant_value: Option<&Key> =
      match self.variant_key() {
        Some(variant_key) => {
          let variant_value: Option<&Key> =
            self.variants.iter()
              .find(|&&(ref k, _)| k == variant_key)
              .map(|&(_, ref v)| v);
          if variant_value.is_none() {
            return State::Complete(
              Complete::Noop(
                format!("Variant key {:?} was not configured in variants.", self.variant_key())
              )
            )
          }
          variant_value
        },
        None => None,
      };

    // If the Subject "is a" or "has a" Product, then we're done.
    if let Some(literal_value) = self.select_literal(&context, &self.subject, variant_value) {
      return State::Complete(Complete::Return(literal_value.clone()));
    }

    // Else, attempt to use a configured task to compute the value.
    let mut dependencies = Vec::new();
    let mut matches: Vec<&Key> = Vec::new();
    for dep_node in context.gen_nodes(&self.subject, self.product(), &self.variants) {
      match context.get(&dep_node) {
        Some(&Complete::Return(ref value)) =>
          matches.push(&value),
        Some(&Complete::Noop(_)) =>
          continue,
        Some(&Complete::Throw(ref msg)) =>
          // NB: propagate thrown exception directly.
          return State::Complete(Complete::Throw(msg.clone())),
        None =>
          dependencies.push(dep_node),
      }
    }

    // If any dependencies were unavailable, wait for them; otherwise, determine whether
    // a value was successfully selected.
    if !dependencies.is_empty() {
      // A dependency has not run yet.
      return State::Waiting(dependencies);
    } else if matches.len() > 0 {
      // TODO: Multiple successful tasks are not currently supported. We should allow for this
      // by adding support for "mergeable" products. see:
      //   https://github.com/pantsbuild/pants/issues/2526
      return State::Complete(
        Complete::Throw(format!("Conflicting values produced for this subject and type: {:?}", matches))
      );
    }

    match matches.pop() {
      Some(matched) =>
        // Statically completed!
        State::Complete(Complete::Return(matched.clone())),
      None =>
        State::Complete(
          Complete::Noop(format!("No source of product {:?} for {:?}.", self.product(), self.subject))
        ),
    }
  }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SelectLiteral {
  subject: Key,
  variants: Variants,
  selector: selectors::SelectLiteral,
}

impl Step for SelectLiteral {
  fn step(&self, _: StepContext) -> State {
    State::Complete(Complete::Return(self.subject.clone()))
  }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SelectVariant {
  subject: Key,
  variants: Variants,
  selector: selectors::SelectVariant,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SelectDependencies {
  subject: Key,
  variants: Variants,
  selector: selectors::SelectDependencies,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SelectProjection {
  subject: Key,
  variants: Variants,
  selector: selectors::SelectProjection,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Task {
  subject: Key,
  product: TypeId,
  variants: Variants,
  func: Key,
  clause: Vec<selectors::Selector>,
}

impl Step for Task {
  fn step(&self, context: StepContext) -> State {
    // Compute dependencies for the Node, or determine whether it is a Noop.
    let mut dependencies = Vec::new();
    let mut dep_values: Vec<&Key> = Vec::new();
    for selector in &self.clause {
      let dep_node =
        Node::create(
          selector.clone(),
          self.subject.clone(),
          self.variants.clone()
        );
      match context.get(&dep_node) {
        Some(&Complete::Return(ref value)) =>
          dep_values.push(&value),
        Some(&Complete::Noop(_)) =>
          if selector.optional() {
            dep_values.push(context.key_none());
          } else {
            return State::Complete(
              Complete::Noop(format!("Was missing (at least) input for {:?}.", selector))
            );
          },
        Some(&Complete::Throw(ref msg)) =>
          // NB: propagate thrown exception directly.
          return State::Complete(Complete::Throw(msg.clone())),
        None =>
          dependencies.push(dep_node),
      }
    }

    if !dependencies.is_empty() {
      // A clause was still waiting on dependencies.
      State::Waiting(dependencies)
    } else {
      // Ready to run!
      State::Runnable(Runnable {
        func: self.func.clone(),
        args: dep_values.into_iter().map(|d| d.clone()).collect(),
      })
    }
  }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Filesystem {
  subject: Key,
  product: TypeId,
  variants: Variants,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Node {
  Select(Select),
  SelectLiteral(SelectLiteral),
  SelectVariant(SelectVariant),
  SelectDependencies(SelectDependencies),
  SelectProjection(SelectProjection),
  Task(Task),
  Filesystem(Filesystem),
}

impl Node {
  pub fn create(selector: Selector, subject: Key, variants: Variants) -> Node {
    match selector {
      Selector::Select(s) =>
        Node::Select(Select {
          subject: subject,
          variants: variants,
          selector: s,
        }),
      Selector::SelectVariant(s) =>
        Node::SelectVariant(SelectVariant {
          subject: subject,
          variants: variants,
          selector: s,
        }),
      Selector::SelectLiteral(s) =>
        // NB: Intentionally ignores subject parameter to provide a literal subject.
        Node::SelectLiteral(SelectLiteral {
          subject: s.subject.clone(),
          variants: variants,
          selector: s,
        }),
      Selector::SelectDependencies(s) =>
        Node::SelectDependencies(SelectDependencies {
          subject: subject,
          variants: variants,
          selector: s,
        }),
      Selector::SelectProjection(s) =>
        Node::SelectProjection(SelectProjection {
          subject: subject,
          variants: variants,
          selector: s,
        }),
    }
  }

  pub fn step(&self, deps: HashMap<&Node, &Complete>, tasks: &Tasks) -> State {
    let context =
      StepContext {
        deps: deps,
        tasks: tasks,
      };
    match self {
      &Node::SelectLiteral(ref n) => n.step(context),
      &Node::Task(ref n) => n.step(context),
      n => panic!("TODO! Need to implement step for: {:?}", n),
    }
  }
}
