# Walrus in a lambda body binds to the lambda's local scope.
# After the lambda call returns, the walrus target must NOT exist
# at module scope.
f = lambda y: (y * 2, (doubled := y * 3))
result = f(5)
print(result)
try:
    print(doubled)
except NameError as e:
    print(f"NameError: {e}")
