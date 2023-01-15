/**
 * Code ad from:
 * https://github.com/cribskip/esp8266_spa/tree/0f76d1a14480109fb25c938233c5b149ede96306
 *
 * Modified to allow for integration testing.  Program is intended to be started on each
 * test execution, with stdin/stdout replacing Serial RX/TX.  stderr is used as a stand-in
 * for mqtt publish messages which we can use to validate test results and debug issues.
 */

#include <stdio.h>
#include <string>
#include <CircularBuffer.h>

#define String std::string

#define VERSION "0.37.4"

#define STRON String("ON").c_str()
#define STROFF String("OFF").c_str()

#define HASSIO false
#define PRODUCTION false

CircularBuffer<uint8_t, 35> Q_in;
CircularBuffer<uint8_t, 35> Q_out;

uint8_t crc8();
void ID_request();
void ID_ack();
void rs485_send();
uint8_t x, i, j;
uint8_t last_state_crc = 0x00;
uint8_t send = 0x00;
uint8_t settemp = 0x00;
uint8_t id = 0x00;
char have_config = 0; //stages: 0-> want it; 1-> requested it; 2-> got it; 3-> further processed it
char have_faultlog = 0; //stages: 0-> want it; 1-> requested it; 2-> got it; 3-> further processed it
char have_filtersettings = 0; //stages: 0-> want it; 1-> requested it; 2-> got it; 3-> further processed it
char ip_settings = 0; //stages: 0-> want it; 1-> requested it; 2-> got it; 3-> further processed it
char wifi_settings = 0; //stages: 0-> want it; 1-> requested it; 2-> got it; 3-> further processed it
char faultlog_minutes = 0; //temp logic so we only get the fault log once per 5 minutes
char filtersettings_minutes = 0; //temp logic so we only get the filter settings once per 5 minutes

struct {
  uint8_t jet1 :2;
  uint8_t jet2 :2;
  uint8_t blower :1;
  uint8_t light :1;
  uint8_t restmode:1;
  uint8_t highrange:1;
  uint8_t padding :2;
  uint8_t hour :5;
  uint8_t minutes :6;
} SpaState;

struct {
  uint8_t pump1 :2; //this could be 1=1 speed; 2=2 speeds
  uint8_t pump2 :2;
  uint8_t pump3 :2;
  uint8_t pump4 :2;
  uint8_t pump5 :2;
  uint8_t pump6 :2;
  uint8_t light1 :1;
  uint8_t light2 :1;
  uint8_t circ :1;
  uint8_t blower :1;
  uint8_t mister :1;
  uint8_t aux1 :1;
  uint8_t aux2 :1;
  uint8_t temp_scale :1; //0 -> Farenheit, 1-> Celcius
} SpaConfig;

struct {
  uint8_t totEntry :5;
  uint8_t currEntry :5;
  uint8_t faultCode :6;
  String faultMessage;
  uint8_t daysAgo :8;
  uint8_t hour :5;
  uint8_t minutes :6;
} SpaFaultLog;

struct {
  uint8_t filt1Hour :5;
  uint8_t filt1Minute :6;
  uint8_t filt1DurationHour :5;
  uint8_t filt1DurationMinute :6;
  uint8_t filt2Enable :1;
  uint8_t filt2Hour :5;
  uint8_t filt2Minute :6;
  uint8_t filt2DurationHour :5;
  uint8_t filt2DurationMinute :6;

} SpaFilterSettings;

uint8_t bitRead(uint8_t byte, unsigned int bit_position) {
  return byte & (1 << bit_position);
}

void Serial_begin(void) {
  freopen(NULL, "rb", stdin);
  freopen(NULL, "rb", stdout);
}

bool Serial_available(void) {
  return true;
}

uint8_t Serial_read(void) {
  uint8_t c = 0;
  fread(&c, sizeof(c), 1, stdin);
  return c;
}

void Serial_write(uint8_t c) {
  fwrite(&c, sizeof(c), 1, stdout);
}

void Serial_flush(void) {
  fflush(stdout);
}

// Original code was using mqtt_publish but if we just print to stderr we can capture that and use it
// for debugging and test validation.
void mqtt_publish(const char* topic, const char* message) {
  fprintf(stderr, "%s:%s", topic, message);
}

void print_msg(CircularBuffer<uint8_t, 35> &data) {
  String s;
  //for (i = 0; i < (Q_in[1] + 2); i++) {
  for (i = 0; i < data.size(); i++) {
    x = Q_in[i];
    if (x < 0x0A) s += "0";
    s += x;
    s += " ";
  }
  mqtt_publish("Spa/node/msg", s.c_str());
}

void decodeFault() {
  SpaFaultLog.totEntry = Q_in[5];
  SpaFaultLog.currEntry = Q_in[6];
  SpaFaultLog.faultCode = Q_in[7];
  switch (SpaFaultLog.faultCode) { // this is a inelegant way to do it, a lookup table would be better
    case 15:
      SpaFaultLog.faultMessage = "Sensors are out of sync";
      break;
    case 16:
      SpaFaultLog.faultMessage = "The water flow is low";
      break;
    case 17:
      SpaFaultLog.faultMessage = "The water flow has failed";
      break;
    case 18:
      SpaFaultLog.faultMessage = "The settings have been reset";
      break;
    case 19:
      SpaFaultLog.faultMessage = "Priming Mode";
      break;
    case 20:
      SpaFaultLog.faultMessage = "The clock has failed";
      break;
    case 21:
      SpaFaultLog.faultMessage = "The settings have been reset";
      break;
    case 22:
      SpaFaultLog.faultMessage = "Program memory failure";
      break;
    case 26:
      SpaFaultLog.faultMessage = "Sensors are out of sync -- Call for service";
      break;
    case 27:
      SpaFaultLog.faultMessage = "The heater is dry";
      break;
    case 28:
      SpaFaultLog.faultMessage = "The heater may be dry";
      break;
    case 29:
      SpaFaultLog.faultMessage = "The water is too hot";
      break;
    case 30:
      SpaFaultLog.faultMessage = "The heater is too hot";
      break;
    case 31:
      SpaFaultLog.faultMessage = "Sensor A Fault";
      break;
    case 32:
      SpaFaultLog.faultMessage = "Sensor B Fault";
      break;
    case 34:
      SpaFaultLog.faultMessage = "A pump may be stuck on";
      break;
    case 35:
      SpaFaultLog.faultMessage = "Hot fault";
      break;
    case 36:
      SpaFaultLog.faultMessage = "The GFCI test failed";
      break;
    case 37:
      SpaFaultLog.faultMessage = "Standby Mode (Hold Mode)";
      break;
    default:
      SpaFaultLog.faultMessage = "Unknown error";
      break;
  }
  SpaFaultLog.daysAgo = Q_in[8];
  SpaFaultLog.hour = Q_in[9];
  SpaFaultLog.minutes = Q_in[10];
  mqtt_publish("Spa/fault/Entries", std::to_string(SpaFaultLog.totEntry).c_str());
  mqtt_publish("Spa/fault/Entry", std::to_string(SpaFaultLog.currEntry).c_str());
  mqtt_publish("Spa/fault/Code", std::to_string(SpaFaultLog.faultCode).c_str());
  mqtt_publish("Spa/fault/Message", SpaFaultLog.faultMessage.c_str());
  mqtt_publish("Spa/fault/DaysAgo", std::to_string(SpaFaultLog.daysAgo).c_str());
  mqtt_publish("Spa/fault/Hours", std::to_string(SpaFaultLog.hour).c_str());
  mqtt_publish("Spa/fault/Minutes", std::to_string(SpaFaultLog.minutes).c_str());
  have_faultlog = 2;
  //mqtt_publish("Spa/debug/have_faultlog", "have the faultlog, #2");
}

void decodeFilterSettings() {
  String s;
  String d;
  String payld;

  SpaFilterSettings.filt1Hour = Q_in[5];
  SpaFilterSettings.filt1Minute = Q_in[6];
  SpaFilterSettings.filt1DurationHour = Q_in[7];
  SpaFilterSettings.filt1DurationMinute = Q_in[8];
  SpaFilterSettings.filt2Enable = bitRead(Q_in[9],7); // check
  SpaFilterSettings.filt2Hour = Q_in[9] ^ (SpaFilterSettings.filt2Enable << 7); // check
  SpaFilterSettings.filt2Minute = Q_in[10];
  SpaFilterSettings.filt2DurationHour = Q_in[11];
  SpaFilterSettings.filt2DurationMinute = Q_in[12];
  //MQTT stuff
  /*mqtt_publish("Spa/config/filt1Hour", String(SpaFilterSettings.filt1Hour).c_str());
  mqtt_publish("Spa/config/filt1Minute", String(SpaFilterSettings.filt1Minute).c_str());
  mqtt_publish("Spa/config/filt1DurationHour", String(SpaFilterSettings.filt1DurationHour).c_str());
  mqtt_publish("Spa/config/filt1DurationMinute", String(SpaFilterSettings.filt1DurationMinute).c_str());
  mqtt_publish("Spa/config/filt2Hour", String(SpaFilterSettings.filt2Hour).c_str());
  mqtt_publish("Spa/config/filt2Minute", String(SpaFilterSettings.filt2Minute).c_str());
  mqtt_publish("Spa/config/filt2DurationHour", String(SpaFilterSettings.filt2DurationHour).c_str());
  mqtt_publish("Spa/config/filt2DurationMinute", String(SpaFilterSettings.filt2DurationMinute).c_str());
  mqtt_publish("Spa/config/filt2Enable", String(SpaFilterSettings.filt2Enable).c_str());*/

  //Filter 1 time conversion
  if (SpaFilterSettings.filt1Hour < 10) s = "0"; else s = "";
  s = std::to_string(SpaFilterSettings.filt1Hour) + ":";
  if (SpaFilterSettings.filt1Minute < 10) s += "0";
  s += std::to_string(SpaFilterSettings.filt1Minute);

  if (SpaFilterSettings.filt1DurationHour < 10) d = "0"; else d = "";
  d = std::to_string(SpaFilterSettings.filt1DurationHour) + ":";
  if (SpaFilterSettings.filt1DurationMinute < 10) d += "0";
  d += std::to_string(SpaFilterSettings.filt1DurationMinute);

  payld = "{\"start\":\""+s+"\",\"duration\":\""+d+"\"}";
  mqtt_publish("Spa/filter1/state", payld.c_str());

  //Filter 2 time conversion
  if (SpaFilterSettings.filt2Hour < 10) s = "0"; else s = "";
  s += std::to_string(SpaFilterSettings.filt2Hour) + ":";
  if (SpaFilterSettings.filt2Minute < 10) s += "0";
  s += std::to_string(SpaFilterSettings.filt2Minute);

  if (SpaFilterSettings.filt2DurationHour < 10) d = "0"; else d = "";
  d += std::to_string(SpaFilterSettings.filt2DurationHour) + ":";
  if (SpaFilterSettings.filt2DurationMinute < 10) d += "0";
  d += std::to_string(SpaFilterSettings.filt2DurationMinute);
  if ((int)(SpaFilterSettings.filt2Enable) == 1) mqtt_publish("Spa/filter2_enabled/state", STRON); else mqtt_publish("Spa/filter2_enabled/state", STROFF);


  payld = "{\"start\":\""+s+"\",\"duration\":\""+d+"\"}";
  mqtt_publish("Spa/filter2/state", payld.c_str());

  have_filtersettings = 2;
}

void decodeSettings() {
  //mqtt_publish("Spa/config/status", "Got config");
  SpaConfig.pump1 = Q_in[5] & 0x03;
  SpaConfig.pump2 = (Q_in[5] & 0x0C) >> 2;
  SpaConfig.pump3 = (Q_in[5] & 0x30) >> 4;
  SpaConfig.pump4 = (Q_in[5] & 0xC0) >> 6;
  SpaConfig.pump5 = (Q_in[6] & 0x03);
  SpaConfig.pump6 = (Q_in[6] & 0xC0) >> 6;
  SpaConfig.light1 = (Q_in[7] & 0x03);
  SpaConfig.light2 = (Q_in[7] >> 2) & 0x03;
  SpaConfig.circ = ((Q_in[8] & 0x80) != 0);
  SpaConfig.blower = ((Q_in[8] & 0x03) != 0);
  SpaConfig.mister = ((Q_in[9] & 0x30) != 0);
  SpaConfig.aux1 = ((Q_in[9] & 0x01) != 0);
  SpaConfig.aux2 = ((Q_in[9] & 0x02) != 0);
  SpaConfig.temp_scale = Q_in[3] & 0x01; //Read temperature scale - 0 -> Farenheit, 1-> Celcius
  mqtt_publish("Spa/config/pumps1", std::to_string(SpaConfig.pump1).c_str());
  mqtt_publish("Spa/config/pumps2", std::to_string(SpaConfig.pump2).c_str());
  mqtt_publish("Spa/config/pumps3", std::to_string(SpaConfig.pump3).c_str());
  mqtt_publish("Spa/config/pumps4", std::to_string(SpaConfig.pump4).c_str());
  mqtt_publish("Spa/config/pumps5", std::to_string(SpaConfig.pump5).c_str());
  mqtt_publish("Spa/config/pumps6", std::to_string(SpaConfig.pump6).c_str());
  mqtt_publish("Spa/config/light1", std::to_string(SpaConfig.light1).c_str());
  mqtt_publish("Spa/config/light2", std::to_string(SpaConfig.light2).c_str());
  mqtt_publish("Spa/config/circ", std::to_string(SpaConfig.circ).c_str());
  mqtt_publish("Spa/config/blower", std::to_string(SpaConfig.blower).c_str());
  mqtt_publish("Spa/config/mister", std::to_string(SpaConfig.mister).c_str());
  mqtt_publish("Spa/config/aux1", std::to_string(SpaConfig.aux1).c_str());
  mqtt_publish("Spa/config/aux2", std::to_string(SpaConfig.aux2).c_str());
  mqtt_publish("Spa/config/temp_scale", std::to_string(SpaConfig.temp_scale).c_str());
  have_config = 2;
}

void decodeState() {
  String s;
  double d = 0.0;
  double c = 0.0;

  // DEBUG for finding meaning:
  //print_msg(Q_in);

  // 25:Flag Byte 20 - Set Temperature
  if (SpaConfig.temp_scale == 0) {
    d = Q_in[25];
  } else if (SpaConfig.temp_scale == 1){
    d = Q_in[25] / 2;
    if (Q_in[25] % 2 == 1) d += 0.5;
  }

  mqtt_publish("Spa/target_temp/state", String(d, 2).c_str());

  // 7:Flag Byte 2 - Actual temperature
  if (Q_in[7] != 0xFF) {
    if (SpaConfig.temp_scale == 0) {
      d = Q_in[7];
    } else if (SpaConfig.temp_scale == 1){
      d = Q_in[7] / 2;
      if (Q_in[7] % 2 == 1) d += 0.5;
    }

    if (c > 0) {
      if ((d > c * 1.2) || (d < c * 0.8)) d = c; //remove spurious readings greater or less than 20% away from previous read
    }

    mqtt_publish("Spa/temperature/state", String(d, 2).c_str());
    c = d;
  } else {
    d = 0;
  }
  // REMARK Move upper publish to HERE to get 0 for unknown temperature

  // 8:Flag Byte 3 Hour & 9:Flag Byte 4 Minute => Time
  if (Q_in[8] < 10) s = "0"; else s = "";
  SpaState.hour = Q_in[8];
  s += std::to_string(Q_in[8]) + ":";
  if (Q_in[9] < 10) s += "0";
  s += std::to_string(Q_in[9]);
  SpaState.minutes = Q_in[9];
  mqtt_publish("Spa/time/state", s.c_str());

  // 10:Flag Byte 5 - Heating Mode
  switch (Q_in[10]) {
    case 0:mqtt_publish("Spa/heatingmode/state", STRON); //Ready
      mqtt_publish("Spa/heat_mode/state", "heat"); //Ready
      SpaState.restmode = 0;
      break;
    case 3:// Ready-in-Rest
      SpaState.restmode = 0;
      break;
    case 1:mqtt_publish("Spa/heatingmode/state", STROFF); //Rest
      mqtt_publish("Spa/heat_mode/state", "off"); //Rest
      SpaState.restmode = 1;
      break;
  }

  // 15:Flags Byte 10 / Heat status, Temp Range
  d = bitRead(Q_in[15], 4);
  if (d == 0) mqtt_publish("Spa/heatstate/state", STROFF);
  else if (d == 1 || d == 2) mqtt_publish("Spa/heatstate/state", STRON);

  d = bitRead(Q_in[15], 2);
  if (d == 0) {
    mqtt_publish("Spa/highrange/state", STROFF); //LOW
    SpaState.highrange = 0;
  } else if (d == 1) {
    mqtt_publish("Spa/highrange/state", STRON); //HIGH
    SpaState.highrange = 1;
  }

  // 16:Flags Byte 11
  if (bitRead(Q_in[16], 1) == 1) {
    mqtt_publish("Spa/jet_1/state", STRON);
    SpaState.jet1 = 1;
  } else {
    mqtt_publish("Spa/jet_1/state", STROFF);
    SpaState.jet1 = 0;
  }

  if (bitRead(Q_in[16], 3) == 1) {
    mqtt_publish("Spa/jet_2/state", STRON);
    SpaState.jet2 = 1;
  } else {
    mqtt_publish("Spa/jet_2/state", STROFF);
    SpaState.jet2 = 0;
  }

  // 18:Flags Byte 13
  if (bitRead(Q_in[18], 1) == 1)
    mqtt_publish("Spa/circ/state", STRON);
  else
    mqtt_publish("Spa/circ/state", STROFF);

  if (bitRead(Q_in[18], 2) == 1) {
    mqtt_publish("Spa/blower/state", STRON);
    SpaState.blower = 1;
  } else {
    mqtt_publish("Spa/blower/state", STROFF);
    SpaState.blower = 0;
  }
  // 19:Flags Byte 14
  if (Q_in[19] == 0x03) {
    mqtt_publish("Spa/light/state", STRON);
    SpaState.light = 1;
  } else {
    mqtt_publish("Spa/light/state", STROFF);
    SpaState.light = 0;
  }

  last_state_crc = Q_in[Q_in[1]];

  // Publish own relay states
  s = "OFF";
  mqtt_publish("Spa/relay_1/state", s.c_str());

  s = "OFF";
  mqtt_publish("Spa/relay_2/state", s.c_str());
}

///////////////////////////////////////////////////////////////////////////////

void mqttpubsub() {
  // ONLY DO THE FOLLOWING IF have_config == true otherwise it will not work
  String Payload;

  // ... Hassio autodiscover
  if (HASSIO) {

      //clear topics:
      mqtt_publish("homeassistant/binary_sensor/Spa", "");
      mqtt_publish("homeassistant/sensor/Spa", "");
      mqtt_publish("homeassistant/switch/Spa", "");
      mqtt_publish("/Spa", "");

      //temperature -> can we try and remove the Payload below, it's messy
      Payload = "{\"name\":\"Hot tub status\",\"uniq_id\":\"ESP82Spa_1\",\"stat_t\":\"Spa/node/state\",\"platform\":\"mqtt\",\"dev\":{\"ids\":[\"ESP82Spa\"],\"name\":\"Esp Spa\",\"sw\":\""+String(VERSION)+"\"}}";
      mqtt_publish("homeassistant/binary_sensor/Spa/state/config", Payload.c_str());
      //climate temperature
      if (SpaConfig.temp_scale == 0) {
        mqtt_publish("homeassistant/climate/Spa/temperature/config", "{\"name\":\"Hot tub thermostat\",\"uniq_id\":\"ESP82Spa_0\",\"temp_cmd_t\":\"Spa/target_temp/set\",\"mode_cmd_t\":\"Spa/heat_mode/set\",\"mode_stat_t\":\"Spa/heat_mode/state\",\"temp_unit\": \"F\",\"curr_temp_t\":\"Spa/temperature/state\",\"temp_stat_t\":\"Spa/target_temp/state\",\"min_temp\":\"80\",\"max_temp\":\"105\",\"modes\":[\"off\", \"heat\"], \"temp_step\":\"1\",\"platform\":\"mqtt\",\"dev\":{\"ids\":[\"ESP82Spa\"]}}");
      } else if (SpaConfig.temp_scale == 1) {
        mqtt_publish("homeassistant/climate/Spa/temperature/config", "{\"name\":\"Hot tub thermostat\",\"uniq_id\":\"ESP82Spa_0\",\"temp_cmd_t\":\"Spa/target_temp/set\",\"mode_cmd_t\":\"Spa/heat_mode/set\",\"mode_stat_t\":\"Spa/heat_mode/state\",\"temp_unit\": \"C\",\"curr_temp_t\":\"Spa/temperature/state\",\"temp_stat_t\":\"Spa/target_temp/state\",\"min_temp\":\"27\",\"max_temp\":\"40\",\"modes\":[\"off\", \"heat\"], \"temp_step\":\"0.5\",\"platform\":\"mqtt\",\"dev\":{\"ids\":[\"ESP82Spa\"]}}");
      }
      //heat mode
      mqtt_publish("homeassistant/switch/Spa/heatingmode/config", "{\"name\":\"Hot tub heating mode\",\"uniq_id\":\"ESP82Spa_3\",\"cmd_t\":\"Spa/heatingmode/set\",\"stat_t\":\"Spa/heatingmode/state\",\"platform\":\"mqtt\",\"dev\":{\"ids\":[\"ESP82Spa\"]}}");
      //heating state
      mqtt_publish("homeassistant/binary_sensor/Spa/heatstate/config", "{\"name\":\"Hot tub heating state\",\"uniq_id\":\"ESP82Spa_6\",\"stat_t\":\"Spa/heatstate/state\",\"platform\":\"mqtt\",\"dev\":{\"ids\":[\"ESP82Spa\"]}}");
      //high range
      mqtt_publish("homeassistant/switch/Spa/highrange/config", "{\"name\":\"Hot tub high range\",\"uniq_id\":\"ESP82Spa_4\",\"cmd_t\":\"Spa/highrange/set\",\"stat_t\":\"Spa/highrange/state\",\"platform\":\"mqtt\",\"dev\":{\"ids\":[\"ESP82Spa\"]}}");

      //OPTIONAL ELEMENTS
      if (SpaConfig.circ){
        //circulation pump
        mqtt_publish("homeassistant/binary_sensor/Spa/circ/config", "{\"name\":\"Hot tub circulation pump\",\"uniq_id\":\"ESP82Spa_5\",\"device_class\":\"power\",\"stat_t\":\"Spa/circ/state\",\"platform\":\"mqtt\",\"dev\":{\"ids\":[\"ESP82Spa\"]}}");
      }
      if (SpaConfig.light1) {
        //light 1
        mqtt_publish("homeassistant/switch/Spa/light/config", "{\"name\":\"Hot tub light\",\"uniq_id\":\"ESP82Spa_7\",\"cmd_t\":\"Spa/light/set\",\"stat_t\":\"Spa/light/state\",\"platform\":\"mqtt\",\"dev\":{\"ids\":[\"ESP82Spa\"]}}");
      }
      if (SpaConfig.pump1 != 0) {
        //jets 1
        mqtt_publish("homeassistant/switch/Spa/jet_1/config", "{\"name\":\"Hot tub jet1\",\"uniq_id\":\"ESP82Spa_8\",\"cmd_t\":\"Spa/jet_1/set\",\"stat_t\":\"Spa/jet_1/state\",\"platform\":\"mqtt\",\"dev\":{\"ids\":[\"ESP82Spa\"]}}");
      }
      if (SpaConfig.pump2 != 0) {
        //jets 2
        mqtt_publish("homeassistant/switch/Spa/jet_2/config", "{\"name\":\"Hot tub jet2\",\"uniq_id\":\"ESP82Spa_9\",\"cmd_t\":\"Spa/jet_2/set\",\"stat_t\":\"Spa/jet_2/state\",\"platform\":\"mqtt\",\"dev\":{\"ids\":[\"ESP82Spa\"]}}");
      }
      if (SpaConfig.blower)
      {
        //blower
        mqtt_publish("homeassistant/switch/Spa/blower/config", "{\"name\":\"Hot tub blower\",\"uniq_id\":\"ESP82Spa_10\",\"cmd_t\":\"Spa/blower/set\",\"stat_t\":\"Spa/blower/state\",\"platform\":\"mqtt\",\"dev\":{\"ids\":[\"ESP82Spa\"]}}");
      }

      mqtt_publish("homeassistant/sensor/Spa/filter1_start/config", "{\"name\":\"Filter 1 start\",\"val_tpl\": \"{{value_json.start}}\",\"uniq_id\":\"ESP82Spa_11\",\"stat_t\":\"Spa/filter1/state\",\"platform\":\"mqtt\",\"dev\":{\"ids\":[\"ESP82Spa\"]}}");
      mqtt_publish("homeassistant/sensor/Spa/filter2_start/config", "{\"name\":\"Filter 2 start\",\"val_tpl\": \"{{value_json.start}}\",\"uniq_id\":\"ESP82Spa_12\",\"stat_t\":\"Spa/filter2/state\",\"platform\":\"mqtt\",\"dev\":{\"ids\":[\"ESP82Spa\"]}}");
      mqtt_publish("homeassistant/sensor/Spa/filter1_duration/config", "{\"name\":\"Filter 1 duration\",\"val_tpl\": \"{{value_json.duration}}\",\"uniq_id\":\"ESP82Spa_13\",\"stat_t\":\"Spa/filter1/state\",\"platform\":\"mqtt\",\"dev\":{\"ids\":[\"ESP82Spa\"]}}");
      mqtt_publish("homeassistant/sensor/Spa/filter2_duration/config", "{\"name\":\"Filter 2 duration\",\"val_tpl\": \"{{value_json.duration}}\",\"uniq_id\":\"ESP82Spa_14\",\"stat_t\":\"Spa/filter2/state\",\"platform\":\"mqtt\",\"dev\":{\"ids\":[\"ESP82Spa\"]}}");
      mqtt_publish("homeassistant/binary_sensor/Spa/filter2_enabled/config", "{\"name\":\"Filter 2 enabled\",\"uniq_id\":\"ESP82Spa_15\",\"stat_t\":\"Spa/filter2_enabled/state\",\"platform\":\"mqtt\",\"dev\":{\"ids\":[\"ESP82Spa\"]}}");
  }

  mqtt_publish("Spa/node/state", "ON");
  mqtt_publish("Spa/node/debug", "RECONNECT");
  //mqtt_publish("Spa/node/debug", String(millis()).c_str());
  //mqtt_publish("Spa/node/debug", String(oldstate).c_str());
  mqtt_publish("Spa/node/version", VERSION);

  //not sure what this is
  last_state_crc = 0x00;

  //done with config
  have_config = 3;
}

/// UPDATE FILESYSTE
void update_started() {
  mqtt_publish("Spa/node/debug", "Updated started");
}

void update_finished() {
  mqtt_publish("Spa/node/debug", "Updated finished");
}

void update_progress(int cur, int total) {
  mqtt_publish("Spa/node/debug", "Update in progress");
}

void update_error(int err) {
  mqtt_publish("Spa/node/debug", "Updated error");
}


///////////////////////////////////////////////////////////////////////////////

void setup() {
  Serial_begin();

  Q_in.clear();
  Q_out.clear();

  /*the below is for debug purposes
  mqtt.connect("Spa1", BROKER_LOGIN.c_str(), BROKER_PASS.c_str());
  mqtt_publish("Spa/debug/wifi_ssid", WIFI_SSID.c_str());
  mqtt_publish("Spa/debug/wifi_password", WIFI_PASSWORD.c_str());
  mqtt_publish("Spa/debug/broker", BROKER.c_str());
  mqtt_publish("Spa/debug/broker_login", BROKER_LOGIN.c_str());
  mqtt_publish("Spa/debug/broker_pass", BROKER_PASS.c_str());
  mqtt_publish("Spa/debug/error", error_msg.c_str());
  */

}

void loop() {
  if (have_config == 2) mqttpubsub(); //do mqtt stuff after we're connected and if we have got the config elements
  //httpServer.handleClient(); needed?

  /*
  ESPhttpUpdate.onStart(update_started);
  ESPhttpUpdate.onEnd(update_finished);
  ESPhttpUpdate.onProgress(update_progress);
  ESPhttpUpdate.onError(update_error);
  */

  //Every x minutes, read the fault log and filter settings using SpaState,minutes, and check for updates
  if ((int)(SpaState.minutes % 5) == 0)
  {
    //logic to only get the error message once -> this is dirty
    //have_faultlog = 0;
    if (have_faultlog == 2) { // we got the fault log before and treated it
      if (faultlog_minutes == SpaState.minutes) { // we got the fault log this interval so do nothing
      }
      else {
        faultlog_minutes = SpaState.minutes;
        have_faultlog = 0;
      }
    }
    if (have_filtersettings == 2) { // we got the filter cycles before and treated it
      if (filtersettings_minutes == SpaState.minutes) { // we got the filter cycles this interval so do nothing
      }
      else {
        filtersettings_minutes = SpaState.minutes;
        have_filtersettings = 0;
      }
    }

    //perform binary updates
    /*
    WiFiClient wifiClient2;
    t_httpUpdate_return ret = ESPhttpUpdate.update(wifiClient2, NEXTVERSION);
    switch (ret) {
      case HTTP_UPDATE_FAILED:
        mqtt_publish("Spa/node/debug", "Update failed");
        break;

      case HTTP_UPDATE_NO_UPDATES:
        mqtt_publish("Spa/node/debug", "No update");
        break;

      case HTTP_UPDATE_OK:
        mqtt_publish("Spa/node/debug", "Update ok");
        break;
    }*/

  }

  // DEBUG:mqtt_publish("Spa/rcv", String(x).c_str()); _yield();
  // Read from Spa RS485
  if (Serial_available()) {
    x = Serial_read();
    Q_in.push(x);

    // Drop until SOF is seen
    if (Q_in.first() != 0x7E) Q_in.clear();
  }

  // Double SOF-marker, drop last one
  if (Q_in[1] == 0x7E && Q_in.size() > 1) Q_in.pop();

  // Complete package
  //if (x == 0x7E && Q_in[0] == 0x7E && Q_in[1] != 0x7E) {
  if (x == 0x7E && Q_in.size() > 2) {
    //print_msg();

    // Unregistered or yet in progress
    if (id == 0) {
      if (Q_in[2] == 0xFE) print_msg(Q_in);

      // FE BF 02:got new client ID
      if (Q_in[2] == 0xFE && Q_in[4] == 0x02) {
        id = Q_in[5];
        if (id > 0x2F) id = 0x2F;

        ID_ack();
        mqtt_publish("Spa/node/id", std::to_string(id).c_str());
      }

      // FE BF 00:Any new clients?
      if (Q_in[2] == 0xFE && Q_in[4] == 0x00) {
        ID_request();
      }
    } else if (Q_in[2] == id && Q_in[4] == 0x06) { // we have an ID, do clever stuff
        // id BF 06:Ready to Send
        if (send == 0xff) {
          // 0xff marks dirty temperature for now
          Q_out.push(id);
          Q_out.push(0xBF);
          Q_out.push(0x20);
          Q_out.push(settemp);
        } else if (send == 0x00) {
          if (have_config == 0) { // Get configuration of the hot tub
            Q_out.push(id);
            Q_out.push(0xBF);
            Q_out.push(0x22);
            Q_out.push(0x00);
            Q_out.push(0x00);
            Q_out.push(0x01);
            //mqtt_publish("Spa/config/status", "Getting config");
            have_config = 1;
          } else if (have_faultlog == 0) { // Get the fault log
            Q_out.push(id);
            Q_out.push(0xBF);
            Q_out.push(0x22);
            Q_out.push(0x20);
            Q_out.push(0xFF);
            Q_out.push(0x00);
            have_faultlog = 1;
            //mqtt_publish("Spa/debug/have_faultlog", "requesting fault log, #1");
          } else if ((have_filtersettings == 0) && (have_faultlog == 2)) { // Get the filter cycles log once we have the faultlog
            Q_out.push(id);
            Q_out.push(0xBF);
            Q_out.push(0x22);
            Q_out.push(0x01);
            Q_out.push(0x00);
            Q_out.push(0x00);
            //mqtt_publish("Spa/debug/have_faultlog", "requesting filter settings, #1");
            have_filtersettings = 1;
          } else {
            // A Nothing to Send message is sent by a client immediately after a Clear to Send message if the client has no messages to send.
            Q_out.push(id);
            Q_out.push(0xBF);
            Q_out.push(0x07);
          }
        } else {
          // Send toggle commands
          Q_out.push(id);
          Q_out.push(0xBF);
          Q_out.push(0x11);
          Q_out.push(send);
          Q_out.push(0x00);
        }

        rs485_send();
        send = 0x00;
    } else if (Q_in[2] == id && Q_in[4] == 0x2E) {
      if (last_state_crc != Q_in[Q_in[1]]) {
        decodeSettings();
      }
    } else if (Q_in[2] == id && Q_in[4] == 0x28) {
      if (last_state_crc != Q_in[Q_in[1]]) {
        decodeFault();
      }
    } else if (Q_in[2] == 0xFF && Q_in[4] == 0x13) { // FF AF 13:Status Update - Packet index offset 5
      if (last_state_crc != Q_in[Q_in[1]]) {
        decodeState();
      }
    } else if (Q_in[2] == id && Q_in[4] == 0x23) { // FF AF 23:Filter Cycle Message - Packet index offset 5
      if (last_state_crc != Q_in[Q_in[1]]) {
        //mqtt_publish("Spa/debug/have_faultlog", "decoding filter settings");
        decodeFilterSettings();
      }
    } else {
      // DEBUG for finding meaning
      //if (Q_in[2] & 0xFE || Q_in[2] == id)
      //print_msg(Q_in);
    }

    // Clean up queue
    Q_in.clear();
  }
}

int main(int argc, char** argv) {
  setup();
  while (true) {
    loop();
  }
  return 0;
}

inline uint8_t crc8(CircularBuffer<uint8_t, 35> &data) {
  unsigned long crc;
  int i, bit;
  uint8_t length = data.size();

  crc = 0x02;
  for ( i = 0 ; i < length ; i++ ) {
    crc ^= data[i];
    for ( bit = 0 ; bit < 8 ; bit++ ) {
      if ( (crc & 0x80) != 0 ) {
        crc <<= 1;
        crc ^= 0x7;
      }
      else {
        crc <<= 1;
      }
    }
  }

  return crc ^ 0x02;
}

inline void ID_request() {
  Q_out.push(0xFE);
  Q_out.push(0xBF);
  Q_out.push(0x01);
  Q_out.push(0x02);
  Q_out.push(0xF1);
  Q_out.push(0x73);

  rs485_send();
}

inline void ID_ack() {
  Q_out.push(id);
  Q_out.push(0xBF);
  Q_out.push(0x03);

  rs485_send();
}

void rs485_send() {
  // Add telegram length
  Q_out.unshift(Q_out.size() + 2);

  // Add CRC
  Q_out.push(crc8(Q_out));

  // Wrap telegram in SOF/EOF
  Q_out.unshift(0x7E);
  Q_out.push(0x7E);

  for (i = 0; i < Q_out.size(); i++)
    Serial_write(Q_out[i]);

  Serial_flush();

  // DEBUG: print_msg(Q_out);
  Q_out.clear();
}
