include("example.inc");
a = 1;
if ((a == 1) && (b = 2)){
  b = 2;
  test(b);
} else if (a == 2){
  b = 3;
  test(b);
}
