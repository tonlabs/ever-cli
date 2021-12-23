pragma ton-solidity >=0.35.0;
pragma AbiHeader expire;
pragma AbiHeader time;
pragma AbiHeader pubkey;
import "Debot.sol";
import "https://raw.githubusercontent.com/tonlabs/DeBot-IS-consortium/main/Terminal/Terminal.sol";
import "https://raw.githubusercontent.com/tonlabs/DeBot-IS-consortium/main/DateTimeInput/DateTimeInput.sol";

contract DateTimeInputDebot is Debot {

    /// @notice Entry point function for DeBot.
    function start() public override {
        DateTimeInput.getDate(tvm.functionId(setDate),
            "Choose a day in 2021 from the begining until current day:", 
            int128(now), 1609448400, int128(now)
        );
    }

    function selectTime() public {
        DateTimeInput.getTime(tvm.functionId(setTime), 
            "Choose a day time (local):", 
            0, 55800, 86100, 1
        );
    }

    function selectDateTime() public {
        DateTimeInput.getDateTime(tvm.functionId(setDateTime), 
            "Choose a day and time (local):",
            int128(now), int128(now), 2609448400, 1, 0x7FFF
        );
    }

    function setDate(int128 date) public {
        if (date == 1234568800) {   
            Terminal.print(tvm.functionId(selectTime), "Date test completed!");
        }
    }

    function setTime(uint32 time) public {
        if (time == 60000) {   
            Terminal.print(tvm.functionId(selectDateTime), "Time test completed!");
        }
    }

    function setDateTime(int128 datetime, int16 timeZoneOffset) public {
        if (datetime == 1234567890) {   
            Terminal.print(0, "Datetime test completed!");
        }
    }

    function getRequiredInterfaces() public view override returns (uint256[] interfaces) {
        return [ Terminal.ID, DateTimeInput.ID ];
    }

    function getDebotInfo() public functionID(0xDEB) override view returns(
        string name, string version, string publisher, string caption, string author,
        address support, string hello, string language, string dabi, bytes icon
    ) {
        name = "DateTimeInput example DeBot";
        version = "0.1.0";
        publisher = "TON Labs";
        caption = "How to use the DateTimeInput interface";
        author = "TON Labs";
        support = address(0);
        hello = "Hello, i am an DateTimeInput example DeBot.";
        language = "en";
        dabi = m_debotAbi.get();
        icon = "";
    }

}