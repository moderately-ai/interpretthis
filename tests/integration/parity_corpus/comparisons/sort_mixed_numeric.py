# sorted() over int/float/bool routes through types::dispatch_lt via
# compare_lt. Pins that the cross-type numeric ordering survives the
# refactor.
print(sorted([3, 1.5, 2, 0, True, False]))
print(sorted([1.5, 1, 2.5, 2], reverse=True))
