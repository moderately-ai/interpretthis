# Pins: ValueError('msg').args is ('msg',) — exception instances
# carry their constructor args as a tuple. Common idiom for code
# that wants to inspect the bare arguments.
e = ValueError('msg')
print(e.args)
print(len(e.args))
print(e.args[0])

e2 = ValueError()
print(e2.args)

e3 = ValueError('a', 'b', 'c')
print(e3.args)
