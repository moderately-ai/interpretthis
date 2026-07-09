# `(x := value)` inside a function body binds x to the function's
# LOCAL scope. After the function returns, x must NOT exist in the
# module scope. The variable-checkpoint refactor's walker must catch
# walrus expressions so they're snapshotted (and removed) on frame
# exit.
def f():
    if (n := 5) > 0:
        return n
    return 0

result = f()
print(result)
# Reading n here should be a NameError — n is local to f.
try:
    print(n)
except NameError as e:
    print(f"NameError: {e}")
