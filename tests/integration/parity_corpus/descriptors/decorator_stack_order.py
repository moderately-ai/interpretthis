# Multiple decorators apply bottom-up: `@a\n@b\ndef f` is equivalent
# to `f = a(b(f))`. Pins the reverse-iteration order in
# eval_function_def's decorator loop.
#
# Uses distinct inner-wrapper names (add_wrap / upper_wrap) rather
# than CPython's idiomatic re-use of `wrapper`, because the body
# cache currently keys by function name and would collide. Tracked by
# gap-function-body-cache-name-collision.
def add_str(fn):
    def add_wrap(x):
        return "add:" + fn(x)
    return add_wrap

def upper_str(fn):
    def upper_wrap(x):
        return fn(x).upper()
    return upper_wrap

@add_str
@upper_str
def greet(name):
    return "hello " + name

# upper_str is applied first (innermost), then add_str wraps it.
# greet("ada") -> add_str(upper_str(greet))("ada")
#             -> "add:" + upper_str(greet)("ada")
#             -> "add:" + greet("ada").upper()
#             -> "add:HELLO ADA"
print(greet("ada"))
