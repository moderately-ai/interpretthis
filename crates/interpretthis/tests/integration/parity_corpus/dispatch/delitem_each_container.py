# Subscript-delete (`__delitem__`) dispatch over list and dict — the two
# builtin mutable containers. Pins types::dispatch_delitem -> per-type
# del_item_slot routing, including the bool-int-unified dict KeyError on
# a second delete.
lst = [10, 20, 30, 40]
del lst[1]
print(lst)                       # [10, 30, 40]
del lst[-1]
print(lst)                       # [10, 30]
d = {"a": 1, "b": 2, "c": 3}
del d["b"]
print(sorted(d.items()))         # [('a', 1), ('c', 3)]
try:
    del d["missing"]
except KeyError:
    print("KeyError")
# Bool-int unification under delete: del d[True] removes the key 1 entry.
d2 = {1: "one", 2: "two"}
del d2[True]
print(d2)                        # {2: 'two'}
