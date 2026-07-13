counter = [0]

def step():
    counter[0] += 1
    return counter[0]

result = []
for v in iter(step, 4):
    result.append(v)
print(result)

stream = [10, 20, 30, 99, 40]
idx = [0]

def pop():
    v = stream[idx[0]]
    idx[0] += 1
    return v

print(list(iter(pop, 99)))
