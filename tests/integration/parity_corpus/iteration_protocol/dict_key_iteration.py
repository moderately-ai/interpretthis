# Iterating a dict yields its keys, matching CPython. Pins dispatch_iter's
# routing to dict_iter (was the legacy direct-match arm).
d = {"a": 1, "b": 2, "c": 3}
for k in d:
    print(k)
print(list(d))
print(sorted(d))                 # explicit sort because dict is insertion-order
print(sum(d.values()))           # values() takes a different path (Vec)
