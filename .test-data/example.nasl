function test(a)
{
  b = a;
  return b;
}
a = 1;
test(a);
{
    c = 2;
    test(c);
}
{
    d = 4;
    test(d);
}
test(c);
