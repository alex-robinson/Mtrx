import AuthenticationServices
import MtrxFFI
import XCTest
@testable import Mtrx

@MainActor
final class SignInModelTests: XCTestCase {
    private var core: FakeMtrx!
    private var browser: FakeBrowser!
    private var signedIn: [Session] = []
    private var model: SignInModel!

    override func setUp() async throws {
        core = FakeMtrx(noHandle: .init())
        browser = FakeBrowser()
        signedIn = []
        model = SignInModel(core: core, browser: browser) { [unowned self] in self.signedIn.append($0) }
    }

    func testPasswordWhenTheServerHasNoBrowserSignIn() async {
        model.checkServer()
        XCTAssertEqual(model.step, .checking)
        await eventually { self.model.step == .password }
    }

    func testBrowserWhenTheServerOffersIt() async {
        core.oauth = true
        model.checkServer()
        await eventually { self.model.step == .browser }
    }

    func testShowsAFailedPasswordSignIn() async {
        model.checkServer()
        await eventually { self.model.step == .password }
        model.user = "me"
        model.password = "wrong"
        model.signInWithPassword()
        XCTAssertTrue(model.signingIn)
        await eventually { !self.model.signingIn }
        XCTAssertEqual(model.error, "Invalid password")
        XCTAssertTrue(signedIn.isEmpty)
    }

    func testSignsInWithAPassword() async {
        let session = FakeSession(noHandle: .init())
        core.loginResult = .success(session)
        model.checkServer()
        await eventually { self.model.step == .password }
        model.user = "me"
        model.password = "secret"
        model.signInWithPassword()
        await eventually { !self.signedIn.isEmpty }
        XCTAssertTrue(signedIn.first === session)
        XCTAssertEqual(model.password, "")
    }

    func testSignsInWithTheBrowser() async {
        core.oauth = true
        let session = FakeSession(noHandle: .init())
        core.oauthLogin.session = session
        browser.result = .success(URL(string: "io.github.alex-robinson:/oauth?code=1&state=2")!)
        model.checkServer()
        await eventually { self.model.step == .browser }
        model.signInWithBrowser()
        await eventually { !self.signedIn.isEmpty }
        XCTAssertEqual(browser.opened?.absoluteString, "https://auth.example.org/login")
        XCTAssertEqual(core.oauthLogin.finishedWith, "io.github.alex-robinson:/oauth?code=1&state=2")
    }

    func testClosingTheBrowserIsNoError() async {
        core.oauth = true
        model.checkServer()
        await eventually { self.model.step == .browser }
        model.signInWithBrowser()
        await eventually { !self.model.signingIn }
        XCTAssertNil(model.error)
        XCTAssertEqual(model.step, .browser)
    }

    func testCancelWhileCheckingGoesBack() {
        model.checkServer()
        model.cancel()
        XCTAssertEqual(model.step, .server)
        XCTAssertFalse(model.signingIn)
    }
}
