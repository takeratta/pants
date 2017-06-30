# coding=utf-8
# Copyright 2014 Pants project contributors (see CONTRIBUTORS.md).
# Licensed under the Apache License, Version 2.0 (see LICENSE).

from __future__ import (absolute_import, division, generators, nested_scopes, print_function,
                        unicode_literals, with_statement)

import hashlib

from pants.backend.jvm.targets.jar_library import JarLibrary
from pants.backend.jvm.targets.jvm_target import JvmTarget
from pants.base.exceptions import TargetDefinitionException
from pants.base.fingerprint_strategy import FingerprintStrategy
from pants.build_graph.address import Address
from pants.build_graph.aliased_target import AliasTarget
from pants.build_graph.resources import Resources
from pants.build_graph.target import Target


class SyntheticTargetNotFound(Exception):
  pass


class DependencyContext(object):
  def __init__(self, compiler_plugin_types, target_closure_kwargs):
    """
    :param compiler_plugins: A dict of compiler plugin target types and their
      additional classpath entries.
    :param target_closure_kwargs: kwargs for the `target.closure` method.
    """
    self.compiler_plugin_types = compiler_plugin_types
    self.target_closure_kwargs = target_closure_kwargs

  @classmethod
  def _get_synthetic_target(cls, target, thrift_dep):
    """Find a thrift target's corresponding synthetic target."""
    for dep in target.dependencies:
      if dep != thrift_dep and dep.is_synthetic and dep.derived_from == thrift_dep:
        return dep
    return None

  @classmethod
  def _resolve_strict_dependencies(cls, target):
    for declared in target.dependencies:
      if type(declared) in (AliasTarget, Target):
        # Is an alias. Recurse to expand.
        for r in cls._resolve_strict_dependencies(declared):
          yield r
      else:
        yield declared

      for export in cls._resolve_exports(declared):
        yield export

  @classmethod
  def _resolve_exports(cls, target):
    for export in getattr(target, 'exports', []):
      if not isinstance(export, Target):
        addr = Address.parse(export, relative_to=target.address.spec_path)
        export = target._build_graph.get_target(addr)
        if export not in target.dependencies:
          # A target can only export its dependencies.
          raise TargetDefinitionException(target, 'Invalid exports: "{}" is not a dependency of {}'.format(export, target))

      if type(export) in (AliasTarget, Target):
        # If exported target is an alias, expand its dependencies.
        for dep in cls._resolve_strict_dependencies(export):
          yield dep
      else:
        if isinstance(export, JavaThriftLibrary):
          synthetic_target = _get_synthetic_target(target, export)
          if synthetic_target is None:
            raise SyntheticTargetNotFound('No synthetic target is found for thrift target: {}'.format(export))
          yield synthetic_target
        else:
          yield export

        for exp in cls._resolve_exports(export):
          yield exp

  def strict_dependencies(self, target):
    """Compute the 'strict' compile target dependencies for this target.

    Results the declared dependencies of a target after alias expansion, with the addition
    of compiler plugins and their transitive deps, since compiletime is actually runtime for them.
    """
    for declared in self._resolve_strict_dependencies(target):
      if isinstance(declared, self.compiler_plugin_types):
        for r in declared.closure(bfs=True, **self.target_closure_kwargs):
          yield r
      else:
        yield declared

  def all_dependencies(self, target):
    """All transitive dependencies of the context's target."""
    for dep in target.closure(bfs=True, **self.target_closure_kwargs):
      yield dep


class ResolvedJarAwareFingerprintStrategy(FingerprintStrategy):
  """Task fingerprint strategy that also includes the resolved coordinates of dependent jars."""

  def __init__(self, classpath_products, dep_context):
    super(ResolvedJarAwareFingerprintStrategy, self).__init__()
    self._classpath_products = classpath_products
    self._dep_context = dep_context

  def compute_fingerprint(self, target):
    if isinstance(target, Resources):
      # Just do nothing, this kind of dependency shouldn't affect result's hash.
      return None

    hasher = hashlib.sha1()
    hasher.update(target.payload.fingerprint())
    if isinstance(target, JarLibrary):
      # NB: Collects only the jars for the current jar_library, and hashes them to ensure that both
      # the resolved coordinates, and the requested coordinates are used. This ensures that if a
      # source file depends on a library with source compatible but binary incompatible signature
      # changes between versions, that you won't get runtime errors due to using an artifact built
      # against a binary incompatible version resolved for a previous compile.
      classpath_entries = self._classpath_products.get_artifact_classpath_entries_for_targets(
        [target])
      for _, entry in classpath_entries:
        hasher.update(str(entry.coordinate))
    return hasher.hexdigest()

  def direct(self, target):
    return target.defaulted_property(lambda x: x.strict_deps)

  def dependencies(self, target):
    if self.direct(target):
      return self._dep_context.strict_dependencies(target)
    return super(ResolvedJarAwareFingerprintStrategy, self).dependencies(target)

  def __hash__(self):
    return hash(type(self))

  def __eq__(self, other):
    return type(self) == type(other)