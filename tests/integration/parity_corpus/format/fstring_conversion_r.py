# Pins: f-string `!r` conversion calls repr() — string gets quoted output.
x = 'hello'
print(f'{x!r}')
