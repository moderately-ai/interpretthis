# Pin: a list reached through a dict subscript must support in-place mutation
# (`.append`) that writes back into the dict cell, not a transient copy.
# Expected stdout: `{1: [5]}`.
groups = {}
groups[1] = []
groups[1].append(5)
print(groups)
