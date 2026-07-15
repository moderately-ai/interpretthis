def squares(n):
    for i in range(n):
        yield i * i
print(sum(squares(5)))
print(max(squares(5)))
print(min(squares(5)))
print(sorted(squares(5), reverse=True))
print(list(filter(lambda x: x > 3, squares(5))))
print(any(x > 10 for x in squares(5)))
print(all(x >= 0 for x in squares(5)))
print(list(map(lambda x: x + 1, squares(4))))
print(", ".join(str(x) for x in squares(4)))
print(next(squares(3)))
g = squares(10)
print([next(g) for _ in range(3)])
print(tuple(squares(3)))
print(9 in squares(5))
print(dict(enumerate(squares(4))))
print([x for x in squares(6) if x % 2 == 0])
