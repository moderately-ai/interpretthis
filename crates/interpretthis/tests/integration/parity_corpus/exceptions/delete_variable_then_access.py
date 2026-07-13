# Pins: deleting a variable then accessing it raises NameError.
x = 42
del x
try:
    print(x)
except NameError:
    print('caught')
