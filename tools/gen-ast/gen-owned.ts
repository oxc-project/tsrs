// Owned payload columns. All field/Kind policy comes from the
// main generator's model; unsupported fields fail generation instead of being
// omitted. Production cutover follows the owner/binder feasibility audit.
import type { Field, Layout } from "./gen-ast.ts";
import type { NodeType } from "./schema.ts";

interface Model {
    nodes: NodeType[];
    layouts: Map<string, Layout>;
    promoted: (layout: Layout) => Field[];
    snake: (name: string) => string;
    canonicalKinds: Map<NodeType, string[]>;
    flagConst: (mask: string) => string;
}

// These domains need explicit owner-backed representations, rather than a
// fabricated static slice or an unowned type-erased checker reference.
const PENDING_DOMAINS = new Map([
    ["SyntheticExpression", "retained semantic payload"],
]);

type OwnedField = Pick<Field, "owner" | "name" | "rust" | "ty" | "cell" | "rare"> & { goOnly: boolean; bitmask?: string };

function semantic(field: OwnedField): boolean {
    return /P<(?:Symbol|SymbolTable|FlowNode|FlowList)>/.test(field.ty);
}

function storageType(field: OwnedField): string {
    switch (field.ty) {
        case "&'static str": return "StoredText";
        case "TokenFlags": return "TokenFlags";
        case "Kind": return "Kind";
        case "bool": return "bool";
        case "&'static [P<Node>]": return "ElementSpan";
        case "&'static [&'static str]": return "StoredTextList";
        case "P<Node>": return "StoredNodeKey";
        case "Option<P<Node>>": return "Option<StoredNodeKey>";
        case "P<NodeList>": return "StoredListKey";
        case "Option<P<NodeList>>": return "Option<StoredListKey>";
        case "Option<P<ModifierList>>": return "Option<StoredModifierListKey>";
        case "Option<P<Symbol>>": return "Option<SymbolIndex>";
        case "Option<P<SymbolTable>>": return "Option<SymbolTableIndex>";
        case "P<FlowNode>": return "FlowIndex";
        case "Option<P<FlowNode>>": return "Option<FlowIndex>";
        case "Option<P<FlowList>>": return "Option<FlowListIndex>";
        case "usize": return "usize";
        default: throw new Error(`Owned prototype requires field support for ${field.owner}.${field.name}: ${field.ty}`);
    }
}

function parameterType(field: OwnedField): string {
    switch (field.ty) {
        case "&'static str": return "TextInput<'_>";
        case "P<Node>": return "NodeKey";
        case "Option<P<Node>>": return "Option<NodeKey>";
        case "P<NodeList>": return "ListKey";
        case "Option<P<NodeList>>": return "Option<ListKey>";
        case "Option<P<ModifierList>>": return "Option<ModifierListKey>";
        case "Option<P<Symbol>>": return "Option<SymbolKey>";
        case "Option<P<SymbolTable>>": return "Option<SymbolTableKey>";
        case "P<FlowNode>": return "FlowKey";
        case "Option<P<FlowNode>>": return "Option<FlowKey>";
        case "Option<P<FlowList>>": return "Option<FlowListKey>";
        case "&'static [P<Node>]": return "&[NodeKey]";
        case "&'static [&'static str]": return "&[TextInput<'_>]";
        default: return storageType(field);
    }
}

function scopedType(field: OwnedField): string {
    return parameterType(field)
        .replaceAll("ModifierListKey", "ScopedModifiers<'brand>")
        .replaceAll("ListKey", "ScopedList<'brand>")
        .replaceAll("NodeKey", "ScopedNode<'brand>");
}

function scopedArgument(field: OwnedField): string {
    const value = field.rust;
    if (field.ty === "&'static [P<Node>]") return `${value}.iter().copied().map(ScopedNode::raw)`;
    if (field.ty.includes("P<Node>") || field.ty.includes("P<NodeList>") || field.ty.includes("P<ModifierList>")) {
        const domain = field.ty.includes("P<NodeList>") ? "ScopedList" : field.ty.includes("P<ModifierList>") ? "ScopedModifiers" : "ScopedNode";
        return field.ty.startsWith("Option<") ? `${value}.map(${domain}::raw)` : `${value}.raw()`;
    }
    return value;
}

function encode(field: OwnedField, value: string): string {
    switch (field.ty) {
        case "&'static str": return `self.texts.encode(&self.source, ${value})?`;
        case "P<Node>": return `self.encode(${value})?`;
        case "Option<P<Node>>": return `${value}.map(|key| self.${field.goOnly ? "encode_node" : "encode"}(key)).transpose()?`;
        case "P<NodeList>": return `self.encode_list(${value})?`;
        case "Option<P<NodeList>>": return `${value}.map(|key| self.encode_list(key)).transpose()?`;
        case "Option<P<ModifierList>>": return `${value}.map(|key| self.encode_modifiers(key)).transpose()?`;
        case "Option<P<Symbol>>": return `${value}.map(|key| bindings.symbol_index(key)).transpose()?`;
        case "Option<P<SymbolTable>>": return `${value}.map(|key| bindings.table_index(key)).transpose()?`;
        case "P<FlowNode>": return `bindings.flow_index(${value})?`;
        case "Option<P<FlowNode>>": return `${value}.map(|key| bindings.flow_index(key)).transpose()?`;
        case "Option<P<FlowList>>": return `${value}.map(|key| bindings.flow_list_index(key)).transpose()?`;
        case "&'static [P<Node>]": return `self.append_list_elements(${value})?`;
        case "&'static [&'static str]": return `self.texts.encode_list(&self.source, ${value})?`;
        default: return value;
    }
}

function decode(field: OwnedField, value: string): string {
    switch (field.ty) {
        case "&'static str": return `${value}.resolve(self.source, self.texts)`;
        case "P<Node>": return `${value}.resolve(self.key.store(), self.foreign)`;
        case "Option<P<Node>>": return `${value}.map(|key| key.resolve(${field.goOnly ? "self.syntax_id" : "self.key.store()"}, self.foreign))`;
        case "P<NodeList>": return `${value}.resolve(self.key.store(), self.foreign_lists)`;
        case "Option<P<NodeList>>": return `${value}.map(|key| key.resolve(self.key.store(), self.foreign_lists))`;
        case "Option<P<ModifierList>>": return `${value}.map(|key| key.resolve(self.key.store(), self.foreign_modifiers))`;
        case "Option<P<Symbol>>": return `${value}.map(|index| SymbolKey::new(self.binding_owner.expect("assigned binding owner"), index))`;
        case "Option<P<SymbolTable>>": return `${value}.map(|index| SymbolTableKey::new(self.binding_owner.expect("assigned binding owner"), index))`;
        case "P<FlowNode>": return `FlowKey::new(self.binding_owner.expect("assigned binding owner"), ${value})`;
        case "Option<P<FlowNode>>": return `${value}.map(|index| FlowKey::new(self.binding_owner.expect("assigned binding owner"), index))`;
        case "Option<P<FlowList>>": return `${value}.map(|index| FlowListKey::new(self.binding_owner.expect("assigned binding owner"), index))`;
        case "&'static [P<Node>]": return `self.lists.raw_nodes(${value}, self.key.store(), self.foreign)`;
        case "&'static [&'static str]": return `self.texts.list(self.source, ${value})`;
        default: return value;
    }
}

function sparseType(field: OwnedField): string {
    const type = storageType(field);
    if (!type.startsWith("Option<") || field.cell) {
        throw new Error(`Sparse fields must be optional and construction-only: ${field.owner}.${field.name}`);
    }
    return type.slice("Option<".length, -1);
}

function needsNodeEdges(fields: OwnedField[]): boolean {
    return fields.some(field => field.ty.includes("P<Node>"));
}

function needsListEdges(fields: OwnedField[]): boolean {
    return fields.some(field => field.ty.includes("P<NodeList>"));
}

function needsText(fields: OwnedField[]): boolean {
    return fields.some(field => field.ty === "&'static str" || field.ty === "&'static [&'static str]");
}

function needsRawNodes(fields: OwnedField[]): boolean {
    return fields.some(field => field.ty === "&'static [P<Node>]");
}

// NewToken allocates canonical payloads with zero values without counting text
// factories. Empty text is the always-valid zero-length source range.
function tokenDefault(field: OwnedField): string {
    if (field.ty.startsWith("Option<")) return "None";
    switch (field.ty) {
        case "&'static str": return "StoredText::EMPTY";
        case "TokenFlags": return "TokenFlags::None";
        case "bool": return "false";
        default: throw new Error(`No token default for ${field.owner}.${field.name}: ${field.ty}`);
    }
}

export function generateOwnedPrototype(model: Model): string {
    const out: string[] = [];
    const w = (s = "") => out.push(s);
    const allTags = [...model.nodes.map(node => node.name), "FlowSwitchClauseData", "FlowReduceLabelData", "CommonOnly"];
    if (allTags.length >= 255) throw new Error("Owned table directory needs a wider slot encoding");
    const entries = model.nodes.filter(node => node.name !== "Token" && !PENDING_DOMAINS.has(node.name)).map(node => {
        const name = node.name;
        const fields: OwnedField[] = model.promoted(model.layouts.get(name)!).map(field => ({
            ...field, goOnly: !!field.member.goOnly,
            bitmask: node.members.find(member => member.name === field.name)?.bitmask,
        }));
        for (const field of fields) {
            storageType(field);
            if (field.rare) sparseType(field);
        }
        const kinds = model.canonicalKinds.get(node)!;
        if (kinds.length === 0) throw new Error(`No canonical kinds for ${name}`);
        const flagMember = node.members.find(member => !member.noFactory && member.type.kind === "primitive" && member.type.name === "NodeFlags");
        const children = node.members.filter(member => !member.noFactory && member.isChild()).map(member => {
            const field = fields.find(field => field.name === member.name);
            if (!field || field.goOnly) throw new Error(`No syntax child field ${name}.${member.name}`);
            return field;
        });
        return { name, fields: fields.filter(field => !field.goOnly), bindingFields: fields.filter(field => field.goOnly), method: model.snake(name), kind: kinds.length === 1 ? kinds[0] : null, kinds, flagMask: flagMember?.bitmask, children, handWrittenVisitor: node.handWrittenVisitor };
    });
    const handwritten = (name: string, fields: [string, string][]) => ({
        name, method: model.snake(name), kind: "Unknown", kinds: ["Unknown"],
        fields: fields.map(([rust, ty]): OwnedField => ({ owner: name, name: rust, rust, ty, cell: false, rare: false, goOnly: false })),
        bindingFields: [] as OwnedField[], flagMask: undefined as string | undefined,
        children: [] as OwnedField[], handWrittenVisitor: false,
    });
    entries.push(
        handwritten("FlowSwitchClauseData", [["switch_statement", "P<Node>"], ["clause_start", "usize"], ["clause_end", "usize"]]),
        handwritten("FlowReduceLabelData", [["target", "P<FlowNode>"], ["antecedents", "Option<P<FlowList>>"]]),
    );
    w("// Code generated by tools/gen-ast/gen-ast.ts and gen-owned.ts. DO NOT EDIT.");
    w("// Owned syntax and guarded binding columns; production factories are not switched yet.");
    w("use tsrs_core::TextRange;");
    w("use rustc_hash::FxHashMap;");
    w("use crate::{Kind, NodeFlags, TokenFlags};");
    w("use super::keys::{NodeTableIndex, StoredListKey, StoredNodeKey, StoredModifierListKey};");
    w("use super::memory::{AstBufferMemory, AstMapMemory, BufferMemory};");
    w("use super::sparse::SparseColumn;");
    w("use super::text::{StoredText, StoredTextList, TextColumns, TextInput, TextListView};");
    w("use super::lists::{ElementSpan, ListColumns, NodeSliceView};");
    w("use super::speculation::{AstScope, ScopeError, ScopedNode, ScopedList, ScopedModifiers, ScopedNodeView, ScopedNodeSlice};");
    w("use super::{AstStoreBuilder, AstStoreError, AstStoreId, CommonColumns, ListKey, ModifierListKey, NodeKey, NodeView, BindingData, SymbolIndex, SymbolKey, SymbolTableIndex, SymbolTableKey, FlowIndex, FlowKey, FlowListIndex, FlowListKey, ChildEdge};");
    w();
    w("#[derive(Clone, Copy, Debug, PartialEq, Eq)]");
    w("#[repr(u8)]");
    w("pub enum PayloadTag {");
    for (const tag of allTags) w(`    ${tag},`);
    w("}");
    w(`pub(super) const TAG_COUNT: usize = ${allTags.length};`);
    w("/// Schema payloads still awaiting an explicit owned field domain.");
    w("pub const PENDING_PAYLOAD_DOMAINS: &[(PayloadTag, &str)] = &[");
    for (const [name, reason] of PENDING_DOMAINS) w(`    (PayloadTag::${name}, ${JSON.stringify(reason)}),`);
    w("];");
    w("impl PayloadTag {");
    w("    pub fn for_kind(kind: Kind) -> Option<Self> {");
    w("        match kind {");
    for (const [node, kinds] of model.canonicalKinds) {
        w(`            ${kinds.map(kind => `Kind::${kind}`).join(" | ")} => Some(Self::${node.name}),`);
    }
    w("            Kind::Count => None,");
    w("        }");
    w("    }");
    w("    pub(super) const fn fixed_kind(self) -> Option<Kind> {");
    w("        match self {");
    const fixedKinds = new Map<string, string[]>();
    for (const { name, kind } of entries) {
        if (kind === null) continue;
        const names = fixedKinds.get(kind) ?? [];
        names.push(name);
        fixedKinds.set(kind, names);
    }
    for (const [kind, names] of fixedKinds) {
        w(`            ${names.map(name => `Self::${name}`).join(" | ")} => Some(Kind::${kind}),`);
    }
    w("            _ => None,");
    w("        }");
    w("    }");
    w("}");
    for (const { name, fields } of entries) {
        w("#[derive(Default)]");
        w(`pub(super) struct ${name}Columns {`);
        w("    common: CommonColumns,");
        for (const field of fields) w(`    ${field.rust}: ${field.rare ? `SparseColumn<${sparseType(field)}>` : `Vec<${storageType(field)}>`},`);
        w("}");
    }
    w("enum Table {");
    w("    CommonOnly(Box<CommonColumns>),");
    w("    Token(Box<CommonColumns>),");
    for (const { name } of entries) w(`    ${name}(Box<${name}Columns>),`);
    w("}");
    w("impl Table {");
    w("    fn common(&self) -> &CommonColumns {");
    w("        match self {");
    w("            Self::CommonOnly(common) | Self::Token(common) => common,");
    for (const { name } of entries) w(`            Self::${name}(columns) => &columns.common,`);
    w("        }");
    w("    }");
    w("    fn common_mut(&mut self) -> &mut CommonColumns {");
    w("        match self {");
    w("            Self::CommonOnly(common) | Self::Token(common) => common,");
    for (const { name } of entries) w(`            Self::${name}(columns) => &mut columns.common,`);
    w("        }");
    w("    }");
    w("    const fn tag(&self) -> PayloadTag {");
    w("        match self {");
    for (const name of ["CommonOnly", "Token", ...entries.map(entry => entry.name)]) w(`            Self::${name}(..) => PayloadTag::${name},`);
    w("        }");
    w("    }");
    w("}");
    w("#[derive(Default)]");
    w("pub(super) struct PayloadColumns { tables: Vec<Table> }");
    w("impl PayloadColumns {");
    w("    pub(super) fn lengths(&self) -> Vec<usize> { self.tables.iter().map(|table| table.common().len()).collect() }");
    w("    pub(super) fn restore(&mut self, lengths: &[usize], release: bool) {");
    w("        self.tables.truncate(lengths.len());");
    w("        for (table, &len) in self.tables.iter_mut().zip(lengths) {");
    w("            match table {");
    w("                Table::CommonOnly(common) | Table::Token(common) => common.restore(len, release),");
    for (const { name, fields } of entries) {
        w(`                Table::${name}(columns) => {`);
        w("                    columns.common.restore(len, release);");
        for (const field of fields) {
            w(field.rare ? `                    columns.${field.rust}.restore(len, release);` : `                    columns.${field.rust}.truncate(len); if release { columns.${field.rust}.shrink_to_fit(); }`);
        }
        w("                }");
    }
    w("            }");
    w("        }");
    w("        if release { self.tables.shrink_to_fit(); }");
    w("    }");
    w("    pub(super) fn new_table(&mut self, tag: PayloadTag) -> Result<NodeTableIndex, AstStoreError> {");
    w("        let index = NodeTableIndex::from_offset(self.tables.len())?;");
    w("        self.tables.reserve(1);");
    w("        let table = match tag {");
    for (const name of ["CommonOnly", "Token", ...entries.map(entry => entry.name)]) w(`            PayloadTag::${name} => Table::${name}(Box::default()),`);
    if (PENDING_DOMAINS.size > 0) {
        w(`            ${[...PENDING_DOMAINS.keys()].map(name => `PayloadTag::${name}`).join(" | ")} => return Err(AstStoreError::UnsupportedPayloadTag(tag)),`);
    }
    w("        };");
    w("        self.tables.push(table);");
    w("        Ok(index)");
    w("    }");
    w("    pub(super) fn common(&self, table: NodeTableIndex) -> &CommonColumns { self.tables[table.offset()].common() }");
    w("    pub(super) fn common_mut(&mut self, table: NodeTableIndex) -> &mut CommonColumns { self.tables[table.offset()].common_mut() }");
    w("    pub(super) fn node(&self, store: AstStoreId, key: NodeKey) -> Result<(&CommonColumns, PayloadTag), AstStoreError> {");
    w("        if key.store() != store { return Err(AstStoreError::WrongStore { expected: store, actual: key.store() }); }");
    w("        let index = key.index();");
    w("        let table = self.tables.get(index.table().offset()).ok_or(AstStoreError::MissingNode(index))?;");
    w("        let common = table.common();");
    w("        if index.element() >= common.len() { return Err(AstStoreError::MissingNode(index)); }");
    w("        Ok((common, table.tag()))");
    w("    }");
    for (const { name, method, fields } of entries) {
        if (fields.length === 0) continue;
        w(`    fn ${method}(&self, table: NodeTableIndex) -> &${name}Columns {`);
        w("        match &self.tables[table.offset()] {");
        w(`            Table::${name}(columns) => columns,`);
        w('            _ => unreachable!("payload tag was checked before selecting the typed table"),');
        w("        }");
        w("    }");
        w(`    fn ${method}_mut(&mut self, table: NodeTableIndex) -> &mut ${name}Columns {`);
        w("        match &mut self.tables[table.offset()] {");
        w(`            Table::${name}(columns) => columns,`);
        w('            _ => unreachable!("payload tag was checked before selecting the typed table"),');
        w("        }");
        w("    }");
    }
    w("}");
    w("impl BufferMemory for PayloadColumns {");
    w("    fn buffer_memory(&self) -> AstBufferMemory {");
    w("        let mut memory = self.tables.buffer_memory();");
    w("        for table in &self.tables {");
    w("            memory = memory + match table {");
    w("                Table::CommonOnly(common) | Table::Token(common) => AstBufferMemory::object(size_of::<CommonColumns>()) + common.buffer_memory(),");
    for (const { name, fields } of entries) {
        w(`                Table::${name}(columns) => AstBufferMemory::object(size_of::<${name}Columns>()) + columns.common.buffer_memory()`);
        for (const field of fields) w(`                    + columns.${field.rust}.buffer_memory()`);
        w("                ,");
    }
    w("            };");
    w("        }");
    w("        memory");
    w("    }");
    w("}");
    w("impl AstStoreBuilder {");
    w("    pub fn new_token(&mut self, kind: Kind, flags: NodeFlags, range: TextRange) -> Result<NodeKey, AstStoreError> {");
    w("        if !crate::is_token_kind(kind) {");
    w("            return Err(AstStoreError::UnsupportedPayload(kind));");
    w("        }");
    const tokenKinds = new Set(model.nodes.find(node => node.name === "Token")!.allKinds().map(kind => kind.name));
    const tokenPayloadKinds = new Map([...model.canonicalKinds].map(([node, kinds]) => [node.name, kinds.filter(kind => tokenKinds.has(kind))]));
    w("        match kind {");
    for (const { name, fields, method } of entries) {
        const canonical = tokenPayloadKinds.get(name) ?? [];
        if (canonical.length === 0) continue;
        w(`            ${canonical.map(kind => `Kind::${kind}`).join(" | ")} => {`);
        w(`                let index = self.prepare_node(PayloadTag::${name})?;`);
        if (fields.length > 0) w(`                let columns = self.payloads.${method}_mut(index.table());`);
        for (const field of fields) w(`                columns.${field.rust}.reserve(${field.rare ? tokenDefault(field) : "1"});`);
        for (const field of fields) w(`                columns.${field.rust}.push(${field.rare ? "index.element(), " : ""}${tokenDefault(field)});`);
        w(`                Ok(self.push_node(index, kind, flags, range, PayloadTag::${name}))`);
        w("            }");
    }
    w("            _ => {");
    w("                let index = self.prepare_node(PayloadTag::Token)?;");
    w("                Ok(self.push_node(index, kind, flags, range, PayloadTag::Token))");
    w("            }");
    w("        }");
    w("    }");
    w("    /// Shallow clone within this builder. Construction resets binding annotations,");
    w("    /// parent, and semantic id; immutable text/list spans keep their identity.");
    w("    pub fn clone_node(&mut self, key: NodeKey) -> Result<NodeKey, AstStoreError> {");
    w("        let node = self.node(key)?;");
    w("        let (kind, flags, range, tag) = (node.kind(), node.flags(), node.range(), node.payload_tag());");
    w("        match tag {");
    w("            PayloadTag::Token => self.new_token(kind, flags, range),");
    for (const { name, method, fields } of entries) {
        w(`            PayloadTag::${name} => {`);
        if (fields.length > 0) {
            w("                let element = key.index().element();");
            w(`                let columns = self.payloads.${method}(key.index().table());`);
            for (const field of fields) w(`                let ${field.rust} = columns.${field.rust}${field.rare ? ".get(element)" : "[element]"};`);
        }
        w(`                let index = self.prepare_node(PayloadTag::${name})?;`);
        if (fields.length > 0) w(`                let columns = self.payloads.${method}_mut(index.table());`);
        for (const field of fields) w(`                columns.${field.rust}.reserve(${field.rare ? field.rust : "1"});`);
        for (const field of fields) w(`                columns.${field.rust}.push(${field.rare ? "index.element(), " : ""}${field.rust});`);
        if (needsText(fields)) w("                self.text_count += 1;");
        w(`                Ok(self.push_node(index, kind, flags, range, PayloadTag::${name}))`);
        w("            }");
    }
    w("            _ => Err(AstStoreError::UnsupportedPayloadTag(tag)),");
    w("        }");
    w("    }");
    for (const { name, fields, method, kind, kinds, flagMask } of entries) {
        const constructed = fields.filter(field => !field.goOnly);
        const needsBindings = constructed.some(semantic);
        const parameters = constructed.map(field => `${field.rust}: ${parameterType(field)}`);
        if (needsBindings) parameters.unshift("bindings: &BindingData");
        if (kind === null) parameters.unshift("kind: Kind");
        w(`    pub fn new_${method}(&mut self, flags: NodeFlags, range: TextRange, ${parameters.join(", ")}) -> Result<NodeKey, AstStoreError> {`);
        if (needsRawNodes(fields)) {
            w(`        self.new_${method}_iter(flags, range, ${[...(kind === null ? ["kind"] : []), ...(needsBindings ? ["bindings"] : []), ...constructed.map(field => field.ty === "&'static [P<Node>]" ? `${field.rust}.iter().copied()` : field.rust)].join(", ")})`);
            w("    }");
            w(`    pub(super) fn new_${method}_iter(&mut self, flags: NodeFlags, range: TextRange, ${parameters.map(parameter => parameter.replace("&[NodeKey]", "impl ExactSizeIterator<Item = NodeKey>")).join(", ")}) -> Result<NodeKey, AstStoreError> {`);
        }
        if (kind === null) w(`        if !matches!(kind, ${kinds.map(kind => `Kind::${kind}`).join(" | ")}) { return Err(AstStoreError::UnsupportedPayload(kind)); }`);
        if (flagMask) w(`        let flags = flags & ${model.flagConst(flagMask)};`);
        if (needsBindings) w("        self.check_bindings(bindings)?;");
        for (const field of fields) {
            if (field.goOnly) { w(`        let ${field.rust} = None;`); continue; }
            const mask = field.bitmask;
            const value = mask ? `${field.rust} & ${model.flagConst(mask)}` : field.rust;
            const encoded = field.ty === "&'static [P<Node>]" ? `self.append_list_elements_iter(${value})?` : encode(field, value);
            if (encoded !== field.rust) w(`        let ${field.rust} = ${encoded};`);
        }
        w(`        let index = self.prepare_node(PayloadTag::${name})?;`);
        if (fields.length > 0) w(`        let columns = self.payloads.${method}_mut(index.table());`);
        for (const field of fields) w(`        columns.${field.rust}.reserve(${field.rare ? field.rust : "1"});`);
        for (const field of fields) w(`        columns.${field.rust}.push(${field.rare ? "index.element(), " : ""}${field.rust});`);
        if (needsText(fields)) w("        self.text_count += 1;");
        w(`        Ok(self.push_node(index, ${kind === null ? "kind" : `Kind::${kind}`}, flags, range, PayloadTag::${name}))`);
        w("    }");
        for (const field of fields.filter(field => field.cell)) {
            w(`    pub fn set_${method}_${field.rust}(&mut self, key: NodeKey, ${semantic(field) ? "bindings: &BindingData, " : ""}value: ${parameterType(field)}) -> Result<(), AstStoreError> {`);
            w(`        let payload = self.payload_offset(key, PayloadTag::${name})?;`);
            if (semantic(field)) w("        self.check_bindings(bindings)?;");
            const encoded = encode(field, "value");
            if (encoded !== "value") w(`        let value = ${encoded};`);
            w(`        self.payloads.${method}_mut(key.index().table()).${field.rust}[payload] = value;`);
            w("        Ok(())");
            w("    }");
        }
    }
    w("}");
    w("impl<'brand> AstScope<'brand, '_> {");
    for (const { fields, method, kind } of entries) {
        const constructed = fields.filter(field => !field.goOnly);
        if (constructed.some(semantic)) continue;
        const parameters = constructed.map(field => `${field.rust}: ${scopedType(field)}`);
        if (kind === null) parameters.unshift("kind: Kind");
        w(`    pub fn new_${method}(&mut self, flags: NodeFlags, range: TextRange, ${parameters.join(", ")}) -> Result<ScopedNode<'brand>, ScopeError> {`);
        w("        self.before_write();");
        const args = [...(kind === null ? ["kind"] : []), ...constructed.map(scopedArgument)];
        w(`        self.writer.new_${method}${needsRawNodes(fields) ? "_iter" : ""}(flags, range, ${args.join(", ")}).map(ScopedNode::wrap).map_err(ScopeError::wrap)`);
        w("    }");
        for (const field of fields.filter(field => field.cell && !semantic(field))) {
            w(`    pub fn set_${method}_${field.rust}(&mut self, key: ScopedNode<'brand>, value: ${scopedType(field)}) -> Result<(), ScopeError> {`);
            w("        self.before_write();");
            if (needsNodeEdges([field]) || needsListEdges([field]) || field.ty.includes("P<ModifierList>")) w("        self.pin_target(key.raw());");
            w(`        self.writer.set_${method}_${field.rust}(key.raw(), ${scopedArgument({ ...field, rust: "value" })}).map_err(ScopeError::wrap)`);
            w("    }");
        }
    }
    w("}");
    w("impl<'brand, 'a> ScopedNodeView<'brand, 'a> {");
    for (const { name, method, fields } of entries) {
        if (fields.some(semantic)) continue;
        w(`    pub fn as_${method}(&self) -> Result<Scoped${name}View<'brand, 'a>, ScopeError> {`);
        w(`        self.inner.as_${method}().map(|inner| Scoped${name}View { inner, brand: std::marker::PhantomData }).map_err(ScopeError::wrap)`);
        w("    }");
    }
    w("}");
    for (const { name, fields } of entries) {
        if (fields.some(semantic)) continue;
        w(`pub struct Scoped${name}View<'brand, 'a> { inner: ${name}View<'a>, brand: std::marker::PhantomData<&'brand mut &'brand ()> }`);
        w(needsText(fields) || needsRawNodes(fields) ? `impl<'brand, 'a> Scoped${name}View<'brand, 'a> {` : `impl<'brand> Scoped${name}View<'brand, '_> {`);
        w("    pub fn key(&self) -> ScopedNode<'brand> { ScopedNode::wrap(self.inner.key()) }");
        for (const field of fields) {
            let result = scopedType(field);
            let value = `self.inner.${field.rust}()`;
            switch (field.ty) {
                case "&'static str": result = "&'a str"; break;
                case "&'static [&'static str]": result = "TextListView<'a>"; break;
                case "&'static [P<Node>]": result = "ScopedNodeSlice<'brand, 'a>"; value = `ScopedNodeSlice::wrap(${value})`; break;
                case "P<Node>": value = `ScopedNode::wrap(${value})`; break;
                case "Option<P<Node>>": value = `${value}.map(ScopedNode::wrap)`; break;
                case "P<NodeList>": value = `ScopedList::wrap(${value})`; break;
                case "Option<P<NodeList>>": value = `${value}.map(ScopedList::wrap)`; break;
                case "Option<P<ModifierList>>": value = `${value}.map(ScopedModifiers::wrap)`; break;
            }
            w(`    pub fn ${field.rust}(&self) -> ${result} { ${value} }`);
        }
        w("}");
    }
    w("impl<'a> NodeView<'a> {");
    w("    pub const fn payload_tag(&self) -> PayloadTag { self.tag }");
    w("    pub(super) fn next_child_edge(&self, member: &mut usize, element: &mut usize) -> Result<Option<ChildEdge>, AstStoreError> {");
    w("        loop {");
    w("            let edge = match self.tag {");
    for (const { name, method, children, handWrittenVisitor } of entries.filter(entry => entry.children.length > 0)) {
        w(`                PayloadTag::${name} => {`);
        w(`                    let data = self.as_${method}()?;`);
        w("                    match *member {");
        for (let index = 0; index < children.length; index++) {
            const field = children[index];
            let access = `data.${field.rust}()`;
            let optional = field.ty.startsWith("Option<");
            if (handWrittenVisitor) {
                if (name !== "JSDocParameterOrPropertyTag") throw new Error(`Missing owned traversal for handwritten ${name}`);
                if (field.name === "name") { access = "if data.is_name_first() { Some(data.name()) } else { data.type_expression() }"; optional = true; }
                if (field.name === "TypeExpression") { access = "if data.is_name_first() { data.type_expression() } else { Some(data.name()) }"; optional = true; }
            }
            if (field.ty === "&'static [P<Node>]") {
                w(`                        ${index} => if let Some(node) = ${access}.get(*element) {`);
                w("                            *element += 1; Some(ChildEdge::Node(node))");
                w("                        } else { *member += 1; *element = 0; None },");
                continue;
            }
            const variant = field.ty.includes("P<NodeList>") ? "List" : field.ty.includes("P<ModifierList>") ? "Modifiers" : "Node";
            const edge = optional ? `(${access}).map(ChildEdge::${variant})` : `Some(ChildEdge::${variant}(${access}))`;
            w(`                        ${index} => { *member += 1; ${edge} },`);
        }
        w("                        _ => return Ok(None),");
        w("                    }");
        w("                }");
    }
    w("                PayloadTag::CommonOnly => return Err(AstStoreError::UnsupportedPayloadTag(self.tag)),");
    w("                _ => return Ok(None),");
    w("            };");
    w("            if edge.is_some() { return Ok(edge); }");
    w("        }");
    w("    }");
    for (const { name, fields, method } of entries) {
        w(`    pub fn as_${method}(&self) -> Result<${name}View<'a>, AstStoreError> {`);
        w(`        self.check_payload(PayloadTag::${name})?;`);
        w(`        Ok(${name}View { key: self.key,`);
        if (fields.length > 0) w(`            offset: self.offset, columns: self.payloads.${method}(self.key.index().table()),`);
        else w("            owner: std::marker::PhantomData,");
        if (needsText(fields)) w("            source: self.source, texts: self.texts,");
        if (needsRawNodes(fields)) w("            lists: self.lists,");
        if (needsNodeEdges(fields)) w("            foreign: self.foreign,");
        if (needsListEdges(fields)) w("            foreign_lists: self.foreign_lists,");
        if (fields.some(field => field.ty.includes("P<ModifierList>"))) w("            foreign_modifiers: self.foreign_modifiers,");
        if (fields.some(semantic)) w("            binding_owner: self.binding_owner,");
        w("        })");
        w("    }");
    }
    w("}");
    for (const { name, fields } of entries) {
        w(`pub struct ${name}View<'a> {`);
        w("    key: NodeKey,");
        if (fields.length > 0) {
            w("    offset: usize,");
            w(`    columns: &'a ${name}Columns,`);
        } else {
            w("    owner: std::marker::PhantomData<&'a ()>,");
        }
        if (needsText(fields)) {
            w("    source: &'a str,");
            w("    texts: &'a TextColumns,");
        }
        if (needsRawNodes(fields)) w("    lists: &'a ListColumns,");
        if (needsNodeEdges(fields)) w("    foreign: &'a [NodeKey],");
        if (needsListEdges(fields)) w("    foreign_lists: &'a [ListKey],");
        if (fields.some(field => field.ty.includes("P<ModifierList>"))) w("    foreign_modifiers: &'a [ModifierListKey],");
        if (fields.some(semantic)) w("    binding_owner: Option<AstStoreId>,");
        w("}");
        w(needsText(fields) || needsRawNodes(fields)
            ? `impl<'a> ${name}View<'a> {` : `impl ${name}View<'_> {`);
        w("    pub const fn key(&self) -> NodeKey { self.key }");
        for (const field of fields) {
            const result = field.ty === "&'static str" ? "&'a str"
                : field.ty === "&'static [P<Node>]" ? "NodeSliceView<'a>"
                : field.ty === "&'static [&'static str]" ? "TextListView<'a>"
                : parameterType(field);
            const value = field.rare ? `self.columns.${field.rust}.get(self.offset)` : `self.columns.${field.rust}[self.offset]`;
            w(`    pub fn ${field.rust}(&self) -> ${result} { ${decode(field, value)} }`);
        }
        w("}");
    }
    w("const _: () = assert!(size_of::<PayloadTag>() == 1);");
    const annotated = entries.filter(entry => entry.bindingFields.length > 0);
    for (const { name, bindingFields } of annotated) {
        w("#[derive(Default)]");
        w(`pub(super) struct ${name}BindingColumns {`);
        for (const field of bindingFields) w(`    ${field.rust}: Vec<${storageType(field)}>,`);
        w("}");
    }
    w("pub(super) enum BindingTable {");
    for (const { name } of annotated) w(`    ${name}(Box<${name}BindingColumns>),`);
    w("}");
    w("#[derive(Default)]");
    w("pub(super) struct BindingAnnotations {");
    w("    base: Vec<Option<BindingTable>>,");
    w("    fragments: FxHashMap<AstStoreId, Vec<Option<BindingTable>>>,");
    w("}");
    w("impl BindingAnnotations {");
    w('#[expect(clippy::iter_over_hash_type, reason = "Allocation totals are independent of fragment iteration order.")]');
    w("    pub(super) fn memory(&self) -> (AstBufferMemory, AstMapMemory) {");
    w("        let mut buffers = self.base.buffer_memory();");
    w("        for tables in self.fragments.values() { buffers = buffers + tables.buffer_memory(); }");
    w("        for table in self.base.iter().chain(self.fragments.values().flatten()).flatten() {");
    w("            buffers = buffers + match table {");
    for (const { name, bindingFields } of annotated) {
        w(`                BindingTable::${name}(columns) => AstBufferMemory::object(size_of::<${name}BindingColumns>())`);
        for (const field of bindingFields) w(`                    + columns.${field.rust}.buffer_memory()`);
        w("                ,");
    }
    w("            };");
    w("        }");
    w("        (buffers, AstMapMemory::of(&self.fragments))");
    w("    }");
    w("    pub(super) fn fragment_stores(&self) -> impl Iterator<Item = AstStoreId> + '_ { self.fragments.keys().copied() }");
    w("    pub(super) fn visit_node_edges<E>(&self, mut visit: impl FnMut(NodeKey) -> Result<(), E>, syntax_id: AstStoreId, foreign: &[NodeKey]) -> Result<(), E> {");
    w("        for table in self.base.iter().chain(self.fragments.values().flatten()).flatten() {");
    w("            match table {");
    for (const { name, bindingFields } of annotated) {
        const edges = bindingFields.filter(field => field.ty === "P<Node>" || field.ty === "Option<P<Node>>");
        if (!edges.length) continue;
        w(`                BindingTable::${name}(columns) => {`);
        for (const field of edges) {
            w(`                    for &key in columns.${field.rust}.iter()${field.ty.startsWith("Option") ? ".flatten()" : ""} { visit(key.resolve(syntax_id, foreign))?; }`);
        }
        w("                }");
    }
    w("                _ => {}");
    w("            }");
    w("        }");
    w("        Ok(())");
    w("    }");
    w("    fn table(&self, root: AstStoreId, key: NodeKey) -> Option<&BindingTable> {");
    w("        let tables = if key.store() == root { &self.base } else { self.fragments.get(&key.store())? };");
    w("        tables.get(key.index().table().offset())?.as_ref()");
    w("    }");
    w("    fn table_mut(&mut self, root: AstStoreId, key: NodeKey, tag: PayloadTag) -> &mut BindingTable {");
    w("        let tables = if key.store() == root { &mut self.base } else { self.fragments.entry(key.store()).or_default() };");
    w("        let table = key.index().table().offset();");
    w("        if tables.len() <= table { tables.resize_with(table + 1, || None); }");
    w("        tables[table].get_or_insert_with(|| match tag {");
    for (const { name } of annotated) w(`            PayloadTag::${name} => BindingTable::${name}(Box::default()),`);
    w('            _ => unreachable!("only annotated payloads create binding tables"),');
    w("        })");
    w("    }");
    w("}");
    w("impl BindingData {");
    for (const { name, method, bindingFields } of annotated) {
        w(`    pub fn ${method}(&self, node: &NodeView<'_>) -> Result<${name}BindingsView<'_>, AstStoreError> {`);
        w(`        self.check_node_binding(node, PayloadTag::${name})?;`);
        w("        let columns = match self.annotations.table(self.syntax_id, node.key) {");
        w(`            Some(BindingTable::${name}(columns)) => Some(&**columns),`);
        w("            None => None,");
        w('            _ => unreachable!("the syntax owner fixes a table payload"),');
        w("        };");
        w(`        Ok(${name}BindingsView { key: node.key, binding_owner: Some(self.id()), columns${needsNodeEdges(bindingFields) ? ", syntax_id: self.syntax_id, foreign: &self.foreign" : ""} })`);
        w("    }");
        for (const field of bindingFields) {
            w(`    pub fn set_${method}_${field.rust}(&mut self, node: &NodeView<'_>, value: ${parameterType(field)}) -> Result<(), AstStoreError> {`);
            w(`        self.check_node_binding(node, PayloadTag::${name})?;`);
            w("        self.track_node(node.key);");
            if (semantic(field)) w("        let bindings: &BindingData = self;");
            w(`        let value = ${encode(field, "value")};`);
            w(`        let BindingTable::${name}(columns) = self.annotations.table_mut(self.syntax_id, node.key, PayloadTag::${name}) else { unreachable!("the syntax owner fixes a table payload") };`);
            w("        let element = node.key.index().element();");
            w(`        if columns.${field.rust}.len() <= element { columns.${field.rust}.resize(element + 1, None); }`);
            w(`        columns.${field.rust}[element] = value;`);
            w("        Ok(())");
            w("    }");
        }
    }
    w("}");
    for (const { name, bindingFields } of annotated) {
        w(`pub struct ${name}BindingsView<'a> { key: NodeKey, binding_owner: Option<AstStoreId>, columns: Option<&'a ${name}BindingColumns>${needsNodeEdges(bindingFields) ? ", syntax_id: AstStoreId, foreign: &'a [NodeKey]" : ""} }`);
        w(`impl ${name}BindingsView<'_> {`);
        w("    pub const fn key(&self) -> NodeKey { self.key }");
        for (const field of bindingFields) {
            const value = `self.columns.and_then(|columns| columns.${field.rust}.get(self.key.index().element()).copied()).flatten()`;
            w(`    pub fn ${field.rust}(&self) -> ${parameterType(field)} { ${decode(field, value)} }`);
        }
        w("}");
    }
    return out.join("\n") + "\n";
}
