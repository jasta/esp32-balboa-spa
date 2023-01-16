Hardware-agnostic library mock spa main board protocol behaviour.  This was done
so that I could bootstrap test with confidence after a couple failed attempts to
try to manually understand the RS485 signal coming out of the Balboa hot tub
main board.  The end goal here is to test it against one of the existing known working implementations at:

https://github.com/ccutrer/balboa_worldwide_app/wiki#serial-protocol
