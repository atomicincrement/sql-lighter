# Plugin Mechanism Analysis

## Overview

SQLite's extensibility comes from its plugin system, which allows developers to:
- Register custom scalar functions
- Register custom aggregate functions
- Add custom collation sequences
- Create virtual tables
- Implement custom full-text search
- Add JSON support
- Extend query capabilities

## Plugin Architecture

### Core Plugin Types

```rust
pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn initialize(&mut self, db: &Database) -> Result<()>;
    fn shutdown(&mut self) -> Result<()>;
}

pub enum PluginType {
    ScalarFunction(Box<dyn ScalarFunction>),
    AggregateFunction(Box<dyn AggregateFunction>),
    Collation(Box<dyn Collation>),
    VirtualTable(Box<dyn VirtualTable>),
    Extension(Box<dyn Plugin>),
}
```

## Scalar Functions

Custom functions that operate on individual values:

```rust
pub trait ScalarFunction: Send + Sync {
    fn name(&self) -> &str;
    fn arg_count(&self) -> Option<i32>; // None = variable args
    fn call(&self, args: Vec<SqlValue>) -> Result<SqlValue>;
    fn is_deterministic(&self) -> bool;
}

pub struct FunctionRegistry {
    functions: HashMap<String, Arc<dyn ScalarFunction>>,
}

impl FunctionRegistry {
    pub fn register(&mut self, func: Box<dyn ScalarFunction>) {
        self.functions.insert(func.name().to_string(), Arc::from(func));
    }
    
    pub fn call(&self, name: &str, args: Vec<SqlValue>) -> Result<SqlValue> {
        let func = self.functions
            .get(name)
            .ok_or(Error::NotImplemented(format!("Function {} not found", name)))?;
        func.call(args)
    }
}

// Example implementations
pub struct UpperFunction;

impl ScalarFunction for UpperFunction {
    fn name(&self) -> &str { "UPPER" }
    
    fn arg_count(&self) -> Option<i32> { Some(1) }
    
    fn call(&self, mut args: Vec<SqlValue>) -> Result<SqlValue> {
        if args.len() != 1 {
            return Err(Error::ExecutionError("UPPER requires 1 argument".into()));
        }
        
        match args.pop().unwrap() {
            SqlValue::Text(s) => Ok(SqlValue::Text(s.to_uppercase())),
            SqlValue::Null => Ok(SqlValue::Null),
            _ => Err(Error::TypeError("UPPER requires text argument".into())),
        }
    }
    
    fn is_deterministic(&self) -> bool { true }
}
```

## Aggregate Functions

Functions that combine multiple rows:

```rust
pub trait AggregateFunction: Send + Sync {
    fn name(&self) -> &str;
    fn arg_count(&self) -> Option<i32>;
    fn init_context(&self) -> Result<Box<dyn AggregateContext>>;
}

pub trait AggregateContext: Send {
    fn step(&mut self, args: Vec<SqlValue>) -> Result<()>;
    fn final_value(&self) -> Result<SqlValue>;
    fn clone_context(&self) -> Box<dyn AggregateContext>;
}

// Example: Custom SUM implementation
pub struct CustomSum;

impl AggregateFunction for CustomSum {
    fn name(&self) -> &str { "CUSTOM_SUM" }
    fn arg_count(&self) -> Option<i32> { Some(1) }
    
    fn init_context(&self) -> Result<Box<dyn AggregateContext>> {
        Ok(Box::new(SumContext { total: 0.0 }))
    }
}

pub struct SumContext {
    total: f64,
}

impl AggregateContext for SumContext {
    fn step(&mut self, args: Vec<SqlValue>) -> Result<()> {
        if let Some(SqlValue::Integer(n)) = args.first() {
            self.total += *n as f64;
        }
        Ok(())
    }
    
    fn final_value(&self) -> Result<SqlValue> {
        Ok(SqlValue::Real(self.total))
    }
    
    fn clone_context(&self) -> Box<dyn AggregateContext> {
        Box::new(SumContext { total: self.total })
    }
}
```

## Collation Sequences

Custom comparison functions for sorting and grouping:

```rust
pub trait Collation: Send + Sync {
    fn name(&self) -> &str;
    fn compare(&self, left: &SqlValue, right: &SqlValue) -> Ordering;
}

pub struct CollationRegistry {
    collations: HashMap<String, Arc<dyn Collation>>,
}

impl CollationRegistry {
    pub fn register(&mut self, collation: Box<dyn Collation>) {
        self.collations.insert(
            collation.name().to_lowercase(),
            Arc::from(collation)
        );
    }
    
    pub fn get(&self, name: &str) -> Option<Arc<dyn Collation>> {
        self.collations.get(&name.to_lowercase()).cloned()
    }
}

// Example: Case-insensitive collation
pub struct NoCase;

impl Collation for NoCase {
    fn name(&self) -> &str { "NOCASE" }
    
    fn compare(&self, left: &SqlValue, right: &SqlValue) -> Ordering {
        let left_str = format!("{:?}", left).to_lowercase();
        let right_str = format!("{:?}", right).to_lowercase();
        left_str.cmp(&right_str)
    }
}

// Example: Reverse order collation
pub struct Reverse;

impl Collation for Reverse {
    fn name(&self) -> &str { "REVERSE" }
    
    fn compare(&self, left: &SqlValue, right: &SqlValue) -> Ordering {
        let left_str = format!("{:?}", left);
        let right_str = format!("{:?}", right);
        right_str.cmp(&left_str) // Note: reversed
    }
}
```

## Virtual Tables

Abstraction for implementing custom table types:

```rust
pub trait VirtualTableModule: Send + Sync {
    fn name(&self) -> &str;
    fn create_table(&self, args: Vec<String>) -> Result<Box<dyn VirtualTable>>;
}

pub trait VirtualTable: Send + Sync {
    fn table_name(&self) -> &str;
    fn column_count(&self) -> usize;
    fn column_name(&self, index: usize) -> Result<String>;
    fn scan(&self, constraints: Vec<Constraint>) -> Result<Box<dyn VirtualTableCursor>>;
    fn insert(&mut self, row: Vec<SqlValue>) -> Result<u64>;
    fn update(&mut self, rowid: u64, row: Vec<SqlValue>) -> Result<()>;
    fn delete(&mut self, rowid: u64) -> Result<()>;
}

pub trait VirtualTableCursor: Send {
    fn column(&self, index: usize) -> Result<SqlValue>;
    fn rowid(&self) -> Result<u64>;
    fn next(&mut self) -> Result<bool>; // Returns true if more rows
    fn eof(&self) -> bool;
}

// Example: CSV virtual table
pub struct CsvModule;

impl VirtualTableModule for CsvModule {
    fn name(&self) -> &str { "csv" }
    
    fn create_table(&self, args: Vec<String>) -> Result<Box<dyn VirtualTable>> {
        if args.is_empty() {
            return Err(Error::ExecutionError("CSV filename required".into()));
        }
        Ok(Box::new(CsvTable::new(&args[0])?))
    }
}

pub struct CsvTable {
    filename: String,
    data: Vec<Vec<String>>,
    headers: Vec<String>,
}

impl CsvTable {
    pub fn new(filename: &str) -> Result<Self> {
        let contents = std::fs::read_to_string(filename)?;
        let mut lines = contents.lines();
        
        let headers = lines
            .next()
            .ok_or(Error::IoError("Empty CSV file".into()))?
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();
        
        let data = lines
            .map(|line| line.split(',').map(|s| s.trim().to_string()).collect())
            .collect();
        
        Ok(Self { filename: filename.to_string(), data, headers })
    }
}

impl VirtualTable for CsvTable {
    fn table_name(&self) -> &str { "csv" }
    
    fn column_count(&self) -> usize { self.headers.len() }
    
    fn column_name(&self, index: usize) -> Result<String> {
        self.headers.get(index)
            .cloned()
            .ok_or(Error::ExecutionError("Column index out of bounds".into()))
    }
    
    fn scan(&self, _constraints: Vec<Constraint>) -> Result<Box<dyn VirtualTableCursor>> {
        Ok(Box::new(CsvCursor {
            data: self.data.clone(),
            index: 0,
        }))
    }
    
    fn insert(&mut self, row: Vec<SqlValue>) -> Result<u64> {
        unimplemented!("CSV insert not supported")
    }
    
    fn update(&mut self, _rowid: u64, _row: Vec<SqlValue>) -> Result<()> {
        unimplemented!("CSV update not supported")
    }
    
    fn delete(&mut self, _rowid: u64) -> Result<()> {
        unimplemented!("CSV delete not supported")
    }
}

pub struct CsvCursor {
    data: Vec<Vec<String>>,
    index: usize,
}

impl VirtualTableCursor for CsvCursor {
    fn column(&self, index: usize) -> Result<SqlValue> {
        if self.index >= self.data.len() {
            return Err(Error::ExecutionError("EOF".into()));
        }
        
        let value = &self.data[self.index][index];
        // Try to parse as number first, fallback to string
        value.parse::<i64>()
            .map(SqlValue::Integer)
            .or_else(|_| value.parse::<f64>().map(SqlValue::Real))
            .or_else(|_| Ok(SqlValue::Text(value.clone())))
    }
    
    fn rowid(&self) -> Result<u64> { Ok(self.index as u64) }
    
    fn next(&mut self) -> Result<bool> {
        if self.index < self.data.len() {
            self.index += 1;
            Ok(self.index < self.data.len())
        } else {
            Ok(false)
        }
    }
    
    fn eof(&self) -> bool { self.index >= self.data.len() }
}

pub struct Constraint {
    pub column: usize,
    pub operator: ConstraintOperator,
    pub value: SqlValue,
}

pub enum ConstraintOperator {
    Equal,
    Greater,
    GreaterOrEqual,
    Less,
    LessOrEqual,
    Like,
    Match,
}
```

## Plugin Registry

Central location for all registered plugins:

```rust
pub struct PluginRegistry {
    scalar_functions: HashMap<String, Arc<dyn ScalarFunction>>,
    aggregate_functions: HashMap<String, Arc<dyn AggregateFunction>>,
    collations: HashMap<String, Arc<dyn Collation>>,
    virtual_table_modules: HashMap<String, Arc<dyn VirtualTableModule>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            scalar_functions: HashMap::new(),
            aggregate_functions: HashMap::new(),
            collations: HashMap::new(),
            virtual_table_modules: HashMap::new(),
        };
        
        // Register built-in functions
        registry.register_builtins();
        registry
    }
    
    pub fn register_scalar_function(&mut self, func: Box<dyn ScalarFunction>) {
        self.scalar_functions.insert(
            func.name().to_uppercase(),
            Arc::from(func)
        );
    }
    
    pub fn register_aggregate_function(&mut self, func: Box<dyn AggregateFunction>) {
        self.aggregate_functions.insert(
            func.name().to_uppercase(),
            Arc::from(func)
        );
    }
    
    pub fn register_collation(&mut self, collation: Box<dyn Collation>) {
        self.collations.insert(
            collation.name().to_lowercase(),
            Arc::from(collation)
        );
    }
    
    pub fn register_virtual_table(&mut self, module: Box<dyn VirtualTableModule>) {
        self.virtual_table_modules.insert(
            module.name().to_lowercase(),
            Arc::from(module)
        );
    }
    
    pub fn call_scalar_function(&self, name: &str, args: Vec<SqlValue>) -> Result<SqlValue> {
        let func = self.scalar_functions
            .get(&name.to_uppercase())
            .ok_or(Error::NotImplemented(format!("Function {} not found", name)))?;
        func.call(args)
    }
    
    pub fn get_aggregate_function(&self, name: &str) -> Option<Arc<dyn AggregateFunction>> {
        self.aggregate_functions.get(&name.to_uppercase()).cloned()
    }
    
    pub fn get_collation(&self, name: &str) -> Option<Arc<dyn Collation>> {
        self.collations.get(&name.to_lowercase()).cloned()
    }
    
    pub fn create_virtual_table(&self, module_name: &str, args: Vec<String>) -> Result<Box<dyn VirtualTable>> {
        let module = self.virtual_table_modules
            .get(&module_name.to_lowercase())
            .ok_or(Error::NotImplemented(format!("Virtual table module {} not found", module_name)))?;
        module.create_table(args)
    }
    
    fn register_builtins(&mut self) {
        // Register built-in scalar functions
        self.register_scalar_function(Box::new(UpperFunction));
        // ... more built-ins
        
        // Register built-in collations
        self.register_collation(Box::new(NoCase));
        self.register_collation(Box::new(Reverse));
    }
}
```

## Plugin Loading

Support for dynamic plugin loading:

```rust
pub struct PluginLoader {
    registry: PluginRegistry,
}

impl PluginLoader {
    pub fn new(registry: PluginRegistry) -> Self {
        Self { registry }
    }
    
    #[cfg(unix)]
    pub fn load_from_path(&mut self, path: &str) -> Result<()> {
        use std::ffi::CString;
        
        let c_path = CString::new(path)?;
        
        unsafe {
            let lib = libc::dlopen(c_path.as_ptr(), libc::RTLD_LAZY);
            if lib.is_null() {
                return Err(Error::IoError("Failed to load plugin".into()));
            }
            
            // Get the plugin entry point
            let init: extern "C" fn(&mut PluginRegistry) -> Result<()> =
                std::mem::transmute(libc::dlsym(lib, b"plugin_init\0".as_ptr() as *const i8));
            
            if init as *const () == std::ptr::null() {
                return Err(Error::ExecutionError("Plugin missing plugin_init".into()));
            }
            
            init(&mut self.registry)?;
        }
        
        Ok(())
    }
}
```

## Integration with Executor

The execution engine uses plugins:

```rust
pub struct ExecutionEngine {
    // ... other fields
    pub plugin_registry: Arc<PluginRegistry>,
}

impl ExecutionEngine {
    fn execute_scalar_function(&self, name: &str, args: Vec<SqlValue>) -> Result<SqlValue> {
        self.plugin_registry.call_scalar_function(name, args)
    }
    
    fn execute_aggregate(&self, name: &str) -> Result<Box<dyn AggregateContext>> {
        self.plugin_registry
            .get_aggregate_function(name)
            .ok_or(Error::NotImplemented(format!("Aggregate {} not found", name)))?
            .init_context()
    }
}
```

## Built-in Plugin System

Pre-loaded plugins for common operations:

```rust
// String functions
impl ScalarFunction for SubstrFunction { /* ... */ }
impl ScalarFunction for LengthFunction { /* ... */ }
impl ScalarFunction for TrimFunction { /* ... */ }

// Math functions
impl ScalarFunction for AbsFunction { /* ... */ }
impl ScalarFunction for RoundFunction { /* ... */ }

// Type functions
impl ScalarFunction for CastFunction { /* ... */ }
impl ScalarFunction for TypeofFunction { /* ... */ }

// Aggregate functions
impl AggregateFunction for CountFunction { /* ... */ }
impl AggregateFunction for SumFunction { /* ... */ }
impl AggregateFunction for AvgFunction { /* ... */ }
impl AggregateFunction for MinFunction { /* ... */ }
impl AggregateFunction for MaxFunction { /* ... */ }
```

## Data Structures Summary

Key Rust types needed:

```rust
// Traits for extensibility
pub trait ScalarFunction { /* ... */ }
pub trait AggregateFunction { /* ... */ }
pub trait AggregateContext { /* ... */ }
pub trait Collation { /* ... */ }
pub trait VirtualTableModule { /* ... */ }
pub trait VirtualTable { /* ... */ }
pub trait VirtualTableCursor { /* ... */ }

// Registry
pub struct PluginRegistry { /* ... */ }

// Loading
pub struct PluginLoader { /* ... */ }
```

## References

- SQLite documentation: https://www.sqlite.org/appfunc.html
- SQLite source: `func.c`, `aggregate.c`, `vtab.c`
- Rust plugin architecture patterns
