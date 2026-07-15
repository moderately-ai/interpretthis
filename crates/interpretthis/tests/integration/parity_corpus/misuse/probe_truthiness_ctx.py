class B:
    def __init__(self, v): self.v = v
    def __bool__(self): return self.v > 0
    def __repr__(self): return f"B{self.v}"
items = [B(1), B(-1), B(2), B(-3), B(4)]
print(list(filter(None, items)))
print([x for x in items if x])
print(len([x for x in items if not x]))
print([x.v for x in items if x])
r = B(5) if B(0) else B(-1)
print(r)
print(B(1) and B(2))
print(B(0) and B(2))
print(B(0) or B(3))
print(B(1) or B(2))
print(not B(0), not B(1))
count = 0
for x in items:
    if x:
        count += 1
print(count)
while_result = []
data = [B(1), B(2), B(0)]
i = 0
while i < len(data) and data[i]:
    while_result.append(data[i].v)
    i += 1
print(while_result)
print([bool(x) for x in items])
d = {"present": B(1), "absent": B(0)}
print({k for k, v in d.items() if v})
