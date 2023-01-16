Code adopted and modified from: [esp8266_spa](https://github.com/cribskip/esp8266_spa/tree/0f76d1a14480109fb25c938233c5b149ede96306)

Modified to allow for integration testing.  Program is intended to be started on each
test execution, with stdin/stdout replacing Serial RX/TX.  stderr is used as a stand-in
for mqtt publish messages which we can use to validate test results and debug issues.

This approach gives us a path for bootstrapped, continuous integration testing:

1. Build mock mainboard app
2. Integration test against existing known working Wi-Fi module app
3. Build custom Wi-Fi module / topside panel app
4. Test custom client app against mock mainboard app

Definitely overkill for this kind of project, but I wanted to take the opportunity to learn how this might look in practice with a Rust embedded project :)
