# The __import__ builtin is the dynamic form of the import statement, routed
# through the same module allow-list. Only the flat top-level form is supported.
m = __import__("math")
print(m.sqrt(16), m.pi)
print(__import__("math").factorial(5))
json_mod = __import__("json")
print(json_mod.dumps({"a": 1}))
io = __import__("io")
buf = io.StringIO()
buf.write("hi")
print(buf.getvalue())
mod = __import__("statistics")
print(mod.mean([1, 2, 3, 4]))
try:
    __import__("nonexistent_module_xyz")
except ModuleNotFoundError as e:
    print("notfound:", e)
try:
    __import__(123)
except TypeError:
    print("type err")
