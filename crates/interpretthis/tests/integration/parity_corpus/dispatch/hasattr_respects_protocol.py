# hasattr() routes through dispatch_getattr_opt — every builtin type's
# attribute table is consulted via one entry point now. Pins that the
# dispatch path matches the per-type method table for the major builtin
# types and returns False for non-attributes.
print(hasattr("hi", "upper"))             # str method
print(hasattr("hi", "missing"))           # not a str method
print(hasattr([1, 2], "append"))          # list method
print(hasattr([1, 2], "missing"))
print(hasattr((1, 2), "count"))           # tuple method
print(hasattr((1, 2), "append"))          # NOT a tuple method
print(hasattr({1, 2}, "add"))             # set method
print(hasattr({1, 2}, "append"))
print(hasattr({"a": 1}, "keys"))          # dict method
print(hasattr({"a": 1}, "a"))             # dict keys are NOT attributes (CPython)
print(hasattr({"a": 1}, "missing"))
print(hasattr(None, "anything"))          # None has no attributes
print(hasattr(42, "missing"))             # int has no exposed attrs
