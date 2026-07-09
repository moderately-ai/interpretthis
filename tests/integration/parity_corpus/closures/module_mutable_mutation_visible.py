# A module-level mutable container should be mutable from inside a
# module-level function via the global reference — `results.append(x)`
# is the canonical pattern. Pinned to make sure the closure-overlay
# clone doesn't shadow the live module list and discard mutations on
# checkpoint.restore.
results = []

def process(x):
    results.append(x)

process(1)
process(2)
process(3)
print(results)
