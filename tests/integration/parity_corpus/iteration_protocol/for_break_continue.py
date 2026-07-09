# Pins: `continue` skips one iteration; `break` exits the for loop.
result = []
for i in range(10):
    if i == 3:
        continue
    if i == 7:
        break
    result.append(i)
print(result)
