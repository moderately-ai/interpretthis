# Many distinct object() sentinels must stay distinct as dict/set keys.
# With a structural key-equality bug, address-hash control-byte collisions
# (~1/128) silently merge distinct empties, so a few hundred keys reliably
# trips it.
objs = [object() for _ in range(500)]
d = {o: i for i, o in enumerate(objs)}
print(len(d))
print(len(set(objs)))
print(all(d[o] == i for i, o in enumerate(objs)))
