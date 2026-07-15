i = 0
result = []
while i < 5:
    result.append(i)
    i += 1
print(result)
n = 10
count = 0
while n > 1:
    n = n // 2 if n % 2 == 0 else 3 * n + 1
    count += 1
print(count)
found = None
i = 0
data = [3, 7, 2, 8, 5]
while i < len(data):
    if data[i] == 8:
        found = i
        break
    i += 1
print(found)
total = 0
x = 1
while True:
    total += x
    x += 1
    if x > 10:
        break
print(total)
attempts = 0
while attempts < 3:
    attempts += 1
else:
    print("while-else", attempts)
values = [1, 2, 3, 0, 4]
i = 0
while i < len(values) and values[i] != 0:
    i += 1
print(i)
