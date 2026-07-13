# User-defined function decorator. Pins eval_function_def's
# decorator-list loop: the decorator is called with the original
# function as its single argument; its return value replaces the
# name binding.
#
# Inner-wrapper name is `doubled_wrap` (not the CPython-idiomatic
# `wrapper`) to dodge the pre-existing inner-function name
# collision in the body cache.
def double_result(fn):
    def doubled_wrap(x):
        return fn(x) * 2
    return doubled_wrap

@double_result
def add_one(x):
    return x + 1

print(add_one(5))               # (5 + 1) * 2 = 12
print(add_one(0))               # (0 + 1) * 2 = 2
