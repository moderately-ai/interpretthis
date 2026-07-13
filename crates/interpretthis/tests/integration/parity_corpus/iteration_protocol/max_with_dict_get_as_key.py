# Pins: a bound method passed as `key=` to `max` / `min` / `sorted` is a
# first-class callable in CPython. `max(d, key=d.get)` returns the key whose
# value is largest. Customer-reported bug — interpretthis currently returns the
# method-marker sentinel as a `str`, then errors `'str' object is not callable`
# when the `key=` machinery tries to invoke it.
monthly_data = {'A': 1, 'B': 2, 'C': 3}
print(max(monthly_data, key=monthly_data.get))
