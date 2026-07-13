# `global x` inside a function makes assignment go to the module
# scope. After the function returns, the mutation must remain
# visible. Previously the per-frame full-clone of `state.variables`
# wiped this on exit; the checkpoint refactor preserves `global`-
# declared names by excluding them from the per-frame snapshot.
counter = 0

def bump():
    global counter
    counter = counter + 1

bump()
bump()
bump()
print(counter)
