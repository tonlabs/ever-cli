pragma ton-solidity >=0.35.0;
pragma AbiHeader expire;
pragma AbiHeader time;
pragma AbiHeader pubkey;
import "Debot.sol";
import "https://raw.githubusercontent.com/tonlabs/DeBot-IS-consortium/main/Terminal/Terminal.sol";
import "https://raw.githubusercontent.com/tonlabs/DeBot-IS-consortium/main/CountryInput/CountryInput.sol";

contract CountryInputDebot is Debot{

    /// @notice Entry point function for DeBot.
    function start() public override {
        string[] permitted = ["RU", "GB", "KZ"]; 
        string[] banned;
        CountryInput.get(tvm.functionId(setCountry), "Select a country:", permitted, banned);
    }

    function setCountry(string value) public {
        if (value == "RU") {   
            Terminal.print(0, "Country test for permitted list completed");
        }
        string[] permitted; 
        string[] banned = ["RU", "GB", "KZ"];
        CountryInput.get(tvm.functionId(setCountryBanned), "Select a country:", permitted, banned);
    }

    function setCountryBanned(string value) public {
        if (value == "ES") {   
            Terminal.print(0, "Country test for banned list completed");
        }
    }

    function getRequiredInterfaces() public view override returns (uint256[] interfaces) {
        return [ Terminal.ID, CountryInput.ID ];
    }

    function getDebotInfo() public functionID(0xDEB) override view returns(
        string name, string version, string publisher, string caption, string author,
        address support, string hello, string language, string dabi, bytes icon
    ) {
        name = "CountryInput example DeBot";
        version = "0.1.0";
        publisher = "TON Labs";
        caption = "How to use the CountryInput interface";
        author = "TON Labs";
        support = address(0);
        hello = "Hello, i am an CountryInput example DeBot.";
        language = "en";
        dabi = m_debotAbi.get();
        icon = "";
    }
}