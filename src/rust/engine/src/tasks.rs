use std::collections::HashMap;

use core::{Field, Function, Key, TypeConstraint, TypeId};
use externs::Externs;
use selectors::{Selector, Select, SelectDependencies, SelectLiteral, SelectProjection, Task};

/**
 * Registry of tasks able to produce each type, along with a few fundamental python
 * types that the engine must be aware of.
 */
pub struct Tasks {
  intrinsics: HashMap<(TypeId,TypeConstraint), Vec<Task>>,
  tasks: HashMap<TypeConstraint, Vec<Task>>,
  pub externs: Externs,
  pub field_name: Field,
  pub field_products: Field,
  pub field_variants: Field,
  pub type_address: TypeConstraint,
  pub type_has_products: TypeConstraint,
  pub type_has_variants: TypeConstraint,
  // Used during the construction of the tasks map.
  preparing: Option<Task>,
}

/**
 * Defines a stateful lifecycle for defining tasks via the C api. Call in order:
 *   1. task_add() - once per task
 *   2. add_*() - zero or more times per task to add input clauses
 *   3. task_end() - once per task
 *
 * Also has a one-shot method for adding an intrinsic Task (which have no Selectors):
 *   1. intrinsic_add()
 *
 * (This protocol was original defined in a Builder, but that complicated the C lifecycle.)
 */
impl Tasks {
  pub fn new(
    externs: Externs,
    field_name: Field,
    field_products: Field,
    field_variants: Field,
    type_address: TypeConstraint,
    type_has_products: TypeConstraint,
    type_has_variants: TypeConstraint,
  ) -> Tasks {
    Tasks {
      intrinsics: HashMap::new(),
      tasks: HashMap::new(),
      externs: externs,
      field_name: field_name,
      field_products: field_products,
      field_variants: field_variants,
      type_address: type_address,
      type_has_products: type_has_products,
      type_has_variants: type_has_variants,
      preparing: None,
    }
  }

  pub fn gen_tasks(&self, subject_type: &TypeId, product: &TypeConstraint) -> Option<&Vec<Task>> {
    // Use intrinsics if available, otherwise use tasks.
    self.intrinsics.get(&(*subject_type, *product)).or(self.tasks.get(product))
  }

  pub fn intrinsic_add(&mut self, func: Function, subject_type: TypeId, product: TypeConstraint) {
    self.intrinsics.entry((subject_type, product))
      .or_insert_with(|| Vec::new())
      .push(
        Task {
          cacheable: false,
          product: product,
          // TODO: need to lift to a constraint.
          clause: vec![Selector::select(subject_type)],
          func: func,
        },
      );
  }

  /**
   * The following methods define the Task registration lifecycle.
   */

  pub fn task_add(&mut self, func: Function, product: TypeConstraint) {
    assert!(
      self.preparing.is_none(),
      "Must `end()` the previous task creation before beginning a new one!"
    );

    self.preparing =
      Some(
        Task {
          cacheable: true,
          product: product,
          clause: Vec::new(),
          func: func,
        }
      );
  }

  pub fn add_select(&mut self, product: TypeConstraint, variant_key: Option<Key>) {
    self.clause(Selector::Select(
      Select { product: product, variant_key: variant_key }
    ));
  }

  pub fn add_select_dependencies(&mut self, product: TypeConstraint, dep_product: TypeConstraint, field: Field, transitive: bool) {
    self.clause(Selector::SelectDependencies(
      SelectDependencies { product: product, dep_product: dep_product, field: field, transitive: transitive }
    ));
  }

  pub fn add_select_projection(&mut self, product: TypeConstraint, projected_subject: TypeId, field: Field, input_product: TypeConstraint) {
    self.clause(Selector::SelectProjection(
      SelectProjection { product: product, projected_subject: projected_subject, field: field, input_product: input_product }
    ));
  }

  pub fn add_select_literal(&mut self, subject: Key, product: TypeConstraint) {
    self.clause(Selector::SelectLiteral(
      SelectLiteral { subject: subject, product: product }
    ));
  }

  fn clause(&mut self, selector: Selector) {
    self.preparing.as_mut()
      .expect("Must `begin()` a task creation before adding clauses!")
      .clause.push(selector);
  }

  pub fn task_end(&mut self) {
    // Move the task from `preparing` to the Tasks map
    let mut task = self.preparing.take().expect("Must `begin()` a task creation before ending it!");
    let mut tasks = self.tasks.entry(task.product.clone()).or_insert_with(|| Vec::new());
    assert!(
      !tasks.contains(&task),
      "Task {:?} was double-registered.",
      task,
    );
    task.clause.shrink_to_fit();
    tasks.push(task);
  }
}