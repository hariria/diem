initSidebarItems({"constant":[["NO_TYPE_ARGUMENTS","Index 0 into the LocalsSignaturePool, which is guaranteed to be an empty list. Used to represent function/struct instantiation with no type arguments – effectively non-generic functions and structs."],["NUMBER_OF_NATIVE_FUNCTIONS",""]],"enum":[["Ability","An `Ability` classifies what operations are permitted for a given type"],["Bytecode","`Bytecode` is a VM instruction of variable size. The type of the bytecode (opcode) defines the size of the bytecode."],["SignatureToken","A `SignatureToken` is a type declaration for a location."],["StructFieldInformation","`StructFieldInformation` indicates whether a struct is native or has user-specified fields"],["Visibility","`Visibility` restricts the accessibility of the associated entity."]],"fn":[["basic_test_module","Create the following module which is convenient in tests: // module  { //     struct Bar { x: u64 } // //     foo() { //     } // }"],["basic_test_script",""],["empty_module","Return the simplest module that will pass the bounds checker"],["empty_script","Return a simple script that contains only a return in the main()"],["self_module_name",""]],"struct":[["AbilitySet","A set of `Ability`s"],["AbilitySetIterator",""],["AddressIdentifierIndex","Index into the `AddressIdentifier` table."],["CodeUnit","A `CodeUnit` is the body of a function. It has the function header and the instruction stream."],["CompiledModule","A `CompiledModule` defines the structure of a module which is the unit of published code."],["CompiledScript","Contains the main function to execute and its dependencies."],["Constant","A `Constant` is a serialized value along with its type. That type will be deserialized by the loader/evauluator"],["ConstantPoolIndex","Index into the `ConstantPool` table."],["FieldDefinition","A `FieldDefinition` is the definition of a field: its name and the field type."],["FieldHandle","A field access info (owner type and offset)"],["FieldHandleIndex","Index into the `FieldHandle` table."],["FieldInstantiation","A complete or partial instantiation of a field (or the type of it)."],["FieldInstantiationIndex","Index into the `FieldInstantiation` table."],["FunctionDefinition","A `FunctionDefinition` is the implementation of a function. It defines the prototype of the function and the function body."],["FunctionDefinitionIndex","Index into the `FunctionDefinition` table."],["FunctionHandle","A `FunctionHandle` is a reference to a function. It is composed by a `ModuleHandle` and the name and signature of that function within the module."],["FunctionHandleIndex","Index into the `FunctionHandle` table."],["FunctionInstantiation","A complete or partial instantiation of a function"],["FunctionInstantiationIndex","Index into the `FunctionInstantiation` table."],["FunctionSignature","A `FunctionSignature` in internally used to create a unique representation of the overall signature as need. Consider deprecated…"],["IdentifierIndex","Index into the `Identifier` table."],["ModuleHandle","A `ModuleHandle` is a reference to a MOVE module. It is composed by an `address` and a `name`."],["ModuleHandleIndex","Index into the `ModuleHandle` table."],["Signature","A `Signature` is the list of locals used by a function."],["SignatureIndex","Index into the `Signature` table."],["SignatureTokenPreorderTraversalIter","An iterator to help traverse the `SignatureToken` in a non-recursive fashion to avoid overflowing the stack."],["SignatureTokenPreorderTraversalIterWithDepth","Alternative preorder traversal iterator for SignatureToken that also returns the depth at each node."],["StructDefInstantiation","A complete or partial instantiation of a generic struct"],["StructDefInstantiationIndex","Index into the `StructInstantiation` table."],["StructDefinition","A `StructDefinition` is a type definition. It either indicates it is native or defines all the user-specified fields declared on the type."],["StructDefinitionIndex","Index into the `StructDefinition` table."],["StructHandle","A `StructHandle` is a reference to a user defined type. It is composed by a `ModuleHandle` and the name of the type within that module."],["StructHandleIndex","Index into the `StructHandle` table."],["StructTypeParameter","A type parameter used in the declaration of a struct."],["TypeSignature","A type definition. `SignatureToken` allows the definition of the set of known types and their composition."]],"type":[["AddressIdentifierPool","The pool of address identifiers (addresses used in ModuleHandles/ModuleIds). Does not include runtime values. Those are placed in the `ConstantPool`"],["CodeOffset","Index into the code stream for a jump. The offset is relative to the beginning of the instruction stream."],["ConstantPool","The pool of `Constant` values"],["IdentifierPool","The pool of identifiers."],["LocalIndex","Index of a local variable in a function."],["MemberCount","Max number of fields in a `StructDefinition`."],["SignaturePool","The pool of `Signature` instances. Every function definition must define the set of locals used and their types."],["TableIndex","Generic index into one of the tables in the binary format."],["TypeParameterIndex","Type parameters are encoded as indices. This index can also be used to lookup the kind of a type parameter in the `FunctionHandle` and `StructHandle`."],["TypeSignaturePool","The pool of `TypeSignature` instances. Those are system and user types used and their composition (e.g. &U64)."]]});