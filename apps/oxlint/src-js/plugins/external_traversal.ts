import { NODE_TYPE_IDS_MAP, NODE_TYPES_COUNT } from "../generated/type_ids.ts";
import { ancestors } from "../generated/walk.js";
import { debugAssert } from "../utils/asserts.ts";
import { EXIT_FLAG, IDENTIFIER_COUNT_INCREMENT, parseSelector, wrapVisitFnWithSelectorMatch } from "./selector.ts";
import { getVisitorKeysForNode } from "./source_code.ts";

import type { VisitorObject } from "../generated/visitor.d.ts";
import type { Node as TraversalNode } from "../generated/types.d.ts";
import type { Node as VisitNode } from "./types.ts";
import type { VisitFn, EnterExit } from "./visitor.ts";

interface VisitProp {
  fn: VisitFn;
  specificity: number;
  selectorStr: string;
}

interface CompilingEntry {
  enter: VisitProp[];
  exit: VisitProp[];
}

export interface ExternalCompiledVisitor {
  byType: Map<string, CompilingEntry>;
  wildcard: CompilingEntry;
  selectors: CompilingEntry;
  cache: Map<string, EnterExit | null>;
}

const DIRECT_VISITOR_NAME_PATTERN = /^[A-Za-z_$][A-Za-z0-9_$]*$/;

function createCompilingEntry(): CompilingEntry {
  return { enter: [], exit: [] };
}

function addVisitProp(entry: CompilingEntry, visitProp: VisitProp, isExit: boolean): void {
  (isExit ? entry.exit : entry.enter).push(visitProp);
}

function isCfgVisitorName(name: string): boolean {
  const typeId = NODE_TYPE_IDS_MAP.get(name);
  return typeId !== undefined && typeId >= NODE_TYPES_COUNT;
}

export function compileExternalVisitors(
  visitors: VisitorObject[],
): ExternalCompiledVisitor | null {
  const byType = new Map<string, CompilingEntry>();
  const wildcard = createCompilingEntry();
  const selectors = createCompilingEntry();
  let hasActiveVisitors = false;

  for (let visitorIndex = 0, visitorsLen = visitors.length; visitorIndex < visitorsLen; visitorIndex++) {
    const visitor = visitors[visitorIndex];
    if (visitor === null || typeof visitor !== "object") {
      throw new TypeError("Visitor returned from `create` method must be an object");
    }

    const keys = Object.keys(visitor);
    if (keys.length === 0) continue;
    hasActiveVisitors = true;

    for (let keyIndex = 0, keysLen = keys.length; keyIndex < keysLen; keyIndex++) {
      const rawName = keys[keyIndex];
      const visitFn = visitor[rawName] as VisitFn;
      if (typeof visitFn !== "function") {
        throw new TypeError(`'${rawName}' property of visitor object is not a function`);
      }

      let name = rawName;
      let specificity = 0;
      const isExit = name.endsWith(":exit");
      if (isExit) {
        name = name.slice(0, -5);
        specificity = EXIT_FLAG;
      }

      if (isCfgVisitorName(name)) {
        throw new Error("CFG listeners are not supported with whole-file custom parsers yet");
      }

      if (name === "*") {
        addVisitProp(wildcard, { fn: visitFn, specificity, selectorStr: name }, isExit);
        continue;
      }

      if (DIRECT_VISITOR_NAME_PATTERN.test(name)) {
        let entry = byType.get(name);
        if (entry === undefined) {
          entry = createCompilingEntry();
          byType.set(name, entry);
        }
        addVisitProp(
          entry,
          {
            fn: visitFn,
            specificity: specificity | IDENTIFIER_COUNT_INCREMENT,
            selectorStr: name,
          },
          isExit,
        );
        continue;
      }

      const selector = parseSelector(name);
      addVisitProp(
        selectors,
        {
          fn: wrapVisitFnWithSelectorMatch(visitFn, selector.esquerySelector),
          specificity: specificity | selector.specificity,
          selectorStr: name,
        },
        isExit,
      );
    }
  }

  if (hasActiveVisitors === false) return null;

  return {
    byType,
    wildcard,
    selectors,
    cache: new Map(),
  };
}

function sortVisitProps(a: VisitProp, b: VisitProp): number {
  const diff = a.specificity - b.specificity;
  if (diff !== 0) return diff;
  return a.selectorStr === b.selectorStr ? 0 : a.selectorStr < b.selectorStr ? -1 : 1;
}

function mergeVisitProps(visitProps: VisitProp[]): VisitFn | null {
  const numVisitFns = visitProps.length;
  if (numVisitFns === 0) return null;
  if (numVisitFns === 1) return visitProps[0]!.fn;

  const sortedVisitProps = [...visitProps].sort(sortVisitProps);
  return (node: VisitNode) => {
    for (let i = 0; i < numVisitFns; i++) sortedVisitProps[i]!.fn(node);
  };
}

function getMergedEntryForType(
  type: string,
  visitor: ExternalCompiledVisitor,
): EnterExit | null {
  const cachedEntry = visitor.cache.get(type);
  if (cachedEntry !== undefined) return cachedEntry;

  const directEntry = visitor.byType.get(type);
  const enter = mergeVisitProps([
    ...(visitor.wildcard.enter),
    ...(directEntry?.enter ?? []),
    ...(visitor.selectors.enter),
  ]);
  const exit = mergeVisitProps([
    ...(visitor.wildcard.exit),
    ...(directEntry?.exit ?? []),
    ...(visitor.selectors.exit),
  ]);

  const mergedEntry = enter === null && exit === null ? null : { enter, exit };
  visitor.cache.set(type, mergedEntry);
  return mergedEntry;
}

function walkExternalNode(
  node: unknown,
  visitors: ExternalCompiledVisitor,
): void {
  if (node == null) return;

  if (Array.isArray(node)) {
    for (let i = 0, len = node.length; i < len; i++) walkExternalNode(node[i], visitors);
    return;
  }

  if (typeof node !== "object") return;

  const type = (node as { type?: unknown }).type;
  if (typeof type !== "string") return;

  const nodeRecord = node as Record<string, unknown> & { type: string };
  const enterExit = getMergedEntryForType(type, visitors);
  let exit: VisitFn | null = null;
  if (enterExit !== null) {
    exit = enterExit.exit;
    enterExit.enter?.(nodeRecord as unknown as VisitNode);
  }

  ancestors.unshift(nodeRecord as unknown as TraversalNode);
  const ancestorsLen = DEBUG ? ancestors.length : 0;

  const keys = getVisitorKeysForNode(nodeRecord);
  for (let i = 0, len = keys.length; i < len; i++) {
    walkExternalNode(nodeRecord[keys[i]!], visitors);
  }

  debugAssert(
    ancestors.length === ancestorsLen,
    `\`ancestors\` is out of sync with external traversal while visiting \`${type}\``,
  );
  ancestors.shift();
  exit?.(nodeRecord as unknown as VisitNode);
}

export function walkExternalProgram(
  program: VisitNode,
  visitors: ExternalCompiledVisitor,
): void {
  walkExternalNode(program, visitors);
}
