# functools.reduce(func, iter[, initial]) folds left.
# Pins the call-back into call_user_function / call_lambda via the
# new async call_function signature.
import functools

def add(a, b):
    return a + b

print(functools.reduce(add, [1, 2, 3, 4]))           # 10
print(functools.reduce(add, [1, 2, 3, 4], 100))      # 110
# With lambda
print(functools.reduce(lambda x, y: x * y, [1, 2, 3, 4]))  # 24
# Empty + initial
print(functools.reduce(add, [], 42))                 # 42
# Single element, no initial
print(functools.reduce(add, [99]))                   # 99
